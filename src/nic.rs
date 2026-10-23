//! Realtek C+ descriptor-ring NIC driver, two chips behind a PCI device-id match (M2.5):
//!   10ec:8136 RTL8101E family (the NC110's wired NIC): MMIO only, r8169-style init. NOT RUNNABLE IN QEMU.
//!   10ec:8139 rtl8139 in C+ mode (QEMU stand-in): port I/O BAR0, or MMIO BAR1 with cmdline `nic=mmio`.
//! Both use the same 16-byte descriptors and ring registers; the per-chip deltas live in nic_regs.rs and are
//! checked against pinned Linux v6.12 sources on the host (tools/test-nic-regs.sh).
//!
//! Safety rules this file keeps (spec rule 4: no disk access, ever):
//! - The only PCI config write is to the matched NIC's command register (low 16 bits; status written as 0).
//! - MMIO base must be a 32-bit-reachable memory BAR, 256-byte aligned, outside multiboot RAM, and unable to
//!   overlap any mass-storage (class 01h, e.g. SATA/AHCI) BAR (span bounded by its alignment, read-only).
//!   Otherwise the NIC is refused.
//! - Bus mastering is OFF across the chip reset and ring programming (iPXE leaves the NIC DMAing), and only
//!   turned on after the rings point at our buffers.
use crate::nic_regs::{self as R, Regs};
use crate::pci::{self, Bdf};
use crate::port::{inb, inl, inw, outb, outl, outw, rdtsc};
use crate::{kprintln, time};
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{fence, Ordering};

const NRX: usize = 16;
const BUFSZ: usize = 1536;

#[repr(C)]
#[derive(Clone, Copy)]
struct Desc {
    opts1: u32,
    opts2: u32,
    addr: u64,
}

#[repr(C, align(256))]
struct Ring<const N: usize>([Desc; N]);

#[repr(C, align(16))]
struct Bufs<const N: usize>([[u8; BUFSZ]; N]);

const Z: Desc = Desc { opts1: 0, opts2: 0, addr: 0 };
static mut TX_DESC: Ring<1> = Ring([Z; 1]);
static mut RX_DESC: Ring<NRX> = Ring([Z; NRX]);
static mut TX_BUF: Bufs<1> = Bufs([[0; BUFSZ]; 1]);
static mut RX_BUF: Bufs<NRX> = Bufs([[0; BUFSZ]; NRX]);

#[derive(Clone, Copy)]
pub enum Bus {
    Pio(u16),
    Mmio(u64),
}

impl Bus {
    unsafe fn r8(self, o: u16) -> u8 {
        match self {
            Bus::Pio(p) => inb(p + o),
            Bus::Mmio(m) => read_volatile((m + o as u64) as *const u8),
        }
    }
    unsafe fn r16(self, o: u16) -> u16 {
        match self {
            Bus::Pio(p) => inw(p + o),
            Bus::Mmio(m) => read_volatile((m + o as u64) as *const u16),
        }
    }
    unsafe fn r32(self, o: u16) -> u32 {
        match self {
            Bus::Pio(p) => inl(p + o),
            Bus::Mmio(m) => read_volatile((m + o as u64) as *const u32),
        }
    }
    unsafe fn w8(self, o: u16, v: u8) {
        match self {
            Bus::Pio(p) => outb(p + o, v),
            Bus::Mmio(m) => write_volatile((m + o as u64) as *mut u8, v),
        }
    }
    unsafe fn w16(self, o: u16, v: u16) {
        match self {
            Bus::Pio(p) => outw(p + o, v),
            Bus::Mmio(m) => write_volatile((m + o as u64) as *mut u16, v),
        }
    }
    unsafe fn w32(self, o: u16, v: u32) {
        match self {
            Bus::Pio(p) => outl(p + o, v),
            Bus::Mmio(m) => write_volatile((m + o as u64) as *mut u32, v),
        }
    }
}

impl core::fmt::Display for Bus {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Bus::Pio(p) => write!(f, "io={:#x}", p),
            Bus::Mmio(m) => write!(f, "mmio={:#x} (UC)", m),
        }
    }
}

pub struct Rtl {
    pub bus: Bus,
    pub regs: &'static Regs,
    pub bdf: Bdf,
    pub mac: [u8; 6],
    rx_next: usize,
    rx_cfg: u32,
    ver: u8, // r8169 RTL_GIGA_MAC_VER_xx (0 = rtl8139 C+)
}

static mut FORCE_PHY_RESET: bool = false;

/// cmdline `phyreset`: always reset the PHY at init (the pre-boot-time behaviour).
pub fn force_phy_reset() {
    unsafe { FORCE_PHY_RESET = true };
}

const PCI_CMD_IO: u32 = 1 << 0;
const PCI_CMD_MEM: u32 = 1 << 1;
const PCI_CMD_BM: u32 = 1 << 2;

/// The one PCI config write: the NIC's command register. Upper half (status, RW1C) written as 0.
fn set_pci_cmd(b: Bdf, set: u32, clear: u32) {
    let cmd = pci::read32(b, 0x04) & 0xFFFF;
    pci::write32(b, 0x04, (cmd & !clear) | set);
}

extern "C" {
    static mut pd: [u64; 2048]; // boot.s: 4 x 512 2-MiB identity entries covering 0..4 GiB
}

#[repr(C, align(4096))]
struct Pt([u64; 512]);
static mut UC_PT: [Pt; 4] = [const { Pt([0; 512]) }; 4];
static mut UC_USED: usize = 0;

/// Framebuffer (multiboot video mode): mark the 2 MiB identity PDEs covering [base, base+len) uncached (PCD|PWT) so
/// pixel writes reach the screen whatever the firmware's MTRRs say. Refused (false) above 4 GiB, over available RAM,
/// or over a PDE already split into 4 KiB pages.
pub fn map_fb(base: u64, len: u64) -> bool {
    if len == 0 || base.checked_add(len).is_none_or(|e| e > 1 << 32) || crate::mb::ram_overlaps(base, len) {
        return false;
    }
    unsafe {
        for i in (base >> 21)..=((base + len - 1) >> 21) {
            let pde = addr_of_mut!(pd[i as usize]);
            let v = read_volatile(pde);
            if v & (1 << 7) == 0 {
                return false;
            }
            write_volatile(pde, v | (1 << 4) | (1 << 3));
        }
        core::arch::asm!("mov {t}, cr3", "mov cr3, {t}", t = out(reg) _, options(nostack));
        core::arch::asm!("wbinvd", options(nostack)); // no write-back line of the old mapping may land on the screen later
    }
    true
}

/// The framebuffer's 2 MiB PDEs write-combining (on) or back to uncached (off). WC = PAT entry 5, which this
/// sets once (default PA5 is WT, unused here): PDE PAT(bit 12)=1, PCD=0, PWT=1. MTRR UC + PAT WC = WC.
pub fn fb_wc(base: u64, len: u64, on: bool) {
    unsafe {
        let (lo, hi): (u32, u32);
        core::arch::asm!("rdmsr", in("ecx") 0x277u32, out("eax") lo, out("edx") hi, options(nostack));
        let pat = (hi as u64) << 32 | lo as u64;
        let want = pat & !(0xFFu64 << 40) | 0x01u64 << 40;
        core::arch::asm!("wbinvd", options(nostack));
        if pat != want {
            core::arch::asm!("wrmsr", in("ecx") 0x277u32, in("eax") want as u32, in("edx") (want >> 32) as u32, options(nostack));
        }
        for i in (base >> 21)..=((base + len - 1) >> 21) {
            let pde = addr_of_mut!(pd[i as usize]);
            let v = read_volatile(pde);
            if v & (1 << 7) == 0 {
                continue;
            }
            let v = v & !((1 << 12) | (1 << 4) | (1 << 3));
            write_volatile(pde, if on { v | (1 << 12) | (1 << 3) } else { v | (1 << 4) | (1 << 3) });
        }
        core::arch::asm!("mov {t}, cr3", "mov cr3, {t}", t = out(reg) _, options(nostack));
        core::arch::asm!("wbinvd", options(nostack));
    }
}

/// Make ONLY the 4 KiB page holding `base` uncached (PCD|PWT): split the 2 MiB identity PDE that covers it into
/// 512 identity 4 KiB entries with the PDE's own flags, so nothing else in that 2 MiB (on the NM10 possibly the
/// AHCI ABAR) changes attributes. Review fix (GLM-5.2 B1): previously the whole 2 MiB went UC. A PDE already split
/// by an earlier call keeps its table; up to 4 regions (NIC, HD Audio, ...). False = out of tables.
pub fn map_uc_4k(base: u64) -> bool {
    unsafe {
        let pde = addr_of_mut!(pd[(base >> 21) as usize]);
        let old = read_volatile(pde);
        let pt: &mut Pt = if old & (1 << 7) == 0 {
            // Already split. boot.s maps everything with 2 MiB pages, so only we split; refuse a table that is not ours.
            let t = old & !0xFFF;
            let first = addr_of!(UC_PT) as u64;
            if t < first || t >= first + 4 * 4096 || (t - first) / 4096 >= UC_USED as u64 {
                return false;
            }
            &mut *(t as *mut Pt)
        } else {
            if UC_USED == 4 {
                return false;
            }
            let pt = &mut (*addr_of_mut!(UC_PT))[UC_USED];
            UC_USED += 1;
            let region = base & !0x1F_FFFF;
            let flags = old & 0x1F & !(1 << 7); // P, RW, US, PWT, PCD from the PDE; PS dropped for a PTE
            for (i, e) in pt.0.iter_mut().enumerate() {
                *e = (region + (i as u64) * 4096) | flags;
            }
            pt
        };
        pt.0[((base >> 12) & 0x1FF) as usize] |= (1 << 4) | (1 << 3);
        fence(Ordering::SeqCst);
        write_volatile(pde, (pt as *const Pt as u64) | 0x3); // present | rw, points at the 4 KiB table
        core::arch::asm!("mov {t}, cr3", "mov cr3, {t}", t = out(reg) _, options(nostack)); // flush incl. the 2 MiB entry
    }
    true
}

/// Validate a memory BAR for use as the NIC register window, then map its 2 MiB page uncached (PCD|PWT).
fn mmio_window(b: Bdf) -> Result<u64, &'static str> {
    let (base, _is64) = pci::first_mem_bar(b).ok_or("no memory BAR")?;
    if base == 0 {
        return Err("memory BAR unassigned (0)");
    }
    if base & 0xFF != 0 || base + 0x100 > 0x1_0000_0000 {
        return Err("memory BAR not 256-byte aligned or not below 4 GiB");
    }
    if crate::ram_overlaps(base, 0x100) {
        return Err("memory BAR overlaps RAM in the multiboot memory map");
    }
    let mut st = [0u64; 16];
    let n = pci::storage_mem_bars(&mut st);
    for &x in &st[..n] {
        kprintln!("nic: mass-storage memory BAR {:#x} (max span {:#x}), NIC window {:#x}+0x100", x, 1u64 << x.trailing_zeros().min(32), base);
    }
    if !R::clear_of_storage(base, &st[..n]) {
        return Err("NIC register window could overlap a storage controller BAR");
    }
    map_uc_4k(base);
    Ok(base)
}

/// Subset of rtl8169_get_mac_version (r8169_main.c:2318-2325, first match wins): the 8101E/8102E/8103E
/// versions whose init this file implements, plus the 8105E (VER_29/30) found in the real NC110 (same generic path;
/// plus rtl_hw_start_8105e_*, see hw_start_8105e). Anything else (8106E, 8402, ...) is refused.
fn xid_version(xid: u32) -> Option<u8> {
    const T: [(u32, u32, u8); 10] = [
        (0x7cf, 0x409, 29), // RTL8105E (r8169_main.c:2316-2317); the real NC110 is XID 0x40a -> VER_30
        (0x7c8, 0x408, 30),
        (0x7cf, 0x349, 8),
        (0x7cf, 0x249, 8),
        (0x7cf, 0x348, 7),
        (0x7cf, 0x248, 7),
        (0x7cf, 0x240, 14), // RTL8401: listed only so it is NOT mistaken for VER_09 below
        (0x7c8, 0x348, 9),
        (0x7c8, 0x248, 9),
        (0x7c8, 0x340, 10),
    ];
    T.iter().find(|&&(m, v, _)| xid & m == v).map(|t| t.2).filter(|&v| v != 14)
}

pub fn probe(prefer_mmio: bool) -> Result<Rtl, &'static str> {
    if let Some(b) = pci::find(0x10EC, 0x8136) {
        return probe_8101e(b);
    }
    if let Some(b) = pci::find(0x10EC, 0x8139) {
        return probe_8139cp(b, prefer_mmio);
    }
    Err("no 10ec:8136 or 10ec:8139 found (bridges walked)")
}

fn probe_8139cp(b: Bdf, mmio: bool) -> Result<Rtl, &'static str> {
    let bus = if mmio {
        set_pci_cmd(b, 0, PCI_CMD_BM);
        let m = mmio_window(b)?;
        set_pci_cmd(b, PCI_CMD_MEM, PCI_CMD_BM);
        Bus::Mmio(m)
    } else {
        set_pci_cmd(b, PCI_CMD_IO, PCI_CMD_BM);
        let bar0 = pci::read32(b, 0x10);
        if bar0 & 1 == 0 {
            return Err("rtl8139 BAR0 is not I/O");
        }
        Bus::Pio((bar0 & !0x3) as u16)
    };
    let mut mac = [0u8; 6];
    for (i, m) in mac.iter_mut().enumerate() {
        *m = unsafe { bus.r8(R::RTL8139CP.mac0 + i as u16) };
    }
    Ok(Rtl { bus, regs: &R::RTL8139CP, bdf: b, mac, rx_next: 0, rx_cfg: 0x0000_E70A, ver: 0 })
}

fn probe_8101e(b: Bdf) -> Result<Rtl, &'static str> {
    set_pci_cmd(b, 0, PCI_CMD_BM | PCI_CMD_IO); // stop any DMA left running by iPXE before anything else
    let m = mmio_window(b)?;
    set_pci_cmd(b, PCI_CMD_MEM, PCI_CMD_BM | PCI_CMD_IO);
    let bus = Bus::Mmio(m);
    let r = &R::RTL8101E;
    let txc = unsafe { bus.r32(r.tx_config) };
    if txc == 0xFFFF_FFFF {
        return Err("TxConfig reads all-ones (PCI read failed, r8169_main.c:5419)");
    }
    let xid = (txc >> 20) & 0xFCF; // r8169_main.c:5422
    let ver = match xid_version(xid) {
        Some(v) => v,
        None => {
            kprintln!("nic: 10ec:8136 XID {:#05x} is not RTL8101E/8102E/8103E/8105E (VER_07..10, 29, 30): refusing", xid);
            return Err("unsupported 10ec:8136 chip version");
        }
    };
    kprintln!("nic: 10ec:8136 XID {:#05x} -> r8169 RTL_GIGA_MAC_VER_{:02}", xid, ver);
    // rtl_init_rxcfg (r8169_main.c:2486-2490 and default 2510): VER_10 FIFO_THRESH|DMA_BURST, VER_07..09 default.
    let rx_cfg = if ver == 10 { R::RX_FIFO_THRESH | R::RX_DMA_BURST } else { R::RX128_INT_EN | R::RX_DMA_BURST };
    let mut mac = [0u8; 6];
    for (i, x) in mac.iter_mut().enumerate() {
        *x = unsafe { bus.r8(r.mac0 + i as u16) };
    }
    Ok(Rtl { bus, regs: r, bdf: b, mac, rx_next: 0, rx_cfg: rx_cfg | R::ACCEPT_BROADCAST | R::ACCEPT_MY_PHYS, ver })
}

impl Rtl {
    pub fn name(&self) -> &'static str {
        self.regs.name
    }

    fn mdio_write(&self, reg: u32, val: u16) -> Result<(), &'static str> {
        // r8169_mdio_write, r8169_main.c:1154-1164: flag set, wait for it to clear (25 x 20 us), then 20 us.
        unsafe { self.bus.w32(self.regs.phyar, R::PHYAR_FLAG | (reg & 0x1F) << 16 | val as u32) };
        for _ in 0..25 {
            time::delay_us(20);
            if unsafe { self.bus.r32(self.regs.phyar) } & R::PHYAR_FLAG == 0 {
                time::delay_us(20);
                return Ok(());
            }
        }
        Err("PHYAR write timeout")
    }

    fn mdio_read(&self, reg: u32) -> Result<u16, &'static str> {
        // r8169_mdio_read, r8169_main.c:1166-1180: flag clear, wait for it to SET, data in 15:0, then 20 us.
        unsafe { self.bus.w32(self.regs.phyar, (reg & 0x1F) << 16) };
        for _ in 0..25 {
            time::delay_us(20);
            let v = unsafe { self.bus.r32(self.regs.phyar) };
            if v & R::PHYAR_FLAG != 0 {
                time::delay_us(20);
                return Ok(v as u16);
            }
        }
        Err("PHYAR read timeout")
    }

    /// PHY: BMCR reset, wait for self-clear, then autoneg enable + restart (no 1000BASE-T on this PHY).
    fn phy_reset(&self) -> Result<(), &'static str> {
        self.mdio_write(0, 0x8000)?;
        let t = time::now_ms();
        while self.mdio_read(0)? & 0x8000 != 0 {
            if time::now_ms() - t > 500 {
                return Err("PHY reset did not self-clear in 500 ms");
            }
        }
        self.mdio_write(0, 0x1200)
    }

    pub fn skip_8105e(&mut self) {
        if self.ver == 29 || self.ver == 30 {
            kprintln!("nic: nic=no8105e: skipping rtl_hw_start_8105e (generic 810x path only)");
            self.ver += 100;
        }
    }

    /// Wake-on-LAN by magic packet for S3, as Linux r8169 does for VER_29/30 (__rtl8169_set_wol, rtl_set_d3_pll_down,
    /// rtl_wol_enable_rx): Config3 MagicPacket, Config5 LanWake, PMCH D3HOT/D3COLD_NO_PLL_DOWN, the receiver keeps
    /// accepting broadcast/multicast/own address, and PME_En in the PCI PM capability (PME_Status cleared). Returns the
    /// PMCSR config offset so the caller can move the NIC to D3hot right before SLP_EN. RTL8105E only.
    pub fn arm_wol(&self) -> Result<u8, &'static str> {
        if !(self.ver == 29 || self.ver == 30) {
            return Err("WoL only wired up for the RTL8105E (VER_29/30)");
        }
        let r = self.regs;
        unsafe {
            self.bus.w8(r.cfg9346, R::CFG9346_UNLOCK);
            self.bus.w8(0x54, self.bus.r8(0x54) | 0x20); // Config3: MagicPacket
            self.bus.w8(0x56, self.bus.r8(0x56) | 0x02); // Config5: LanWake
            self.bus.w8(r.cfg9346, R::CFG9346_LOCK);
            self.bus.w8(0x6F, self.bus.r8(0x6F) | 0xC0); // PMCH: D3COLD_NO_PLL_DOWN | D3HOT_NO_PLL_DOWN
            self.bus.w32(r.rx_config, self.bus.r32(r.rx_config) | 0x0E); // AcceptBroadcast | AcceptMulticast | AcceptMyPhys
        }
        // PCI power management capability: PMCSR = cap + 4; bit 8 PME_En, bit 15 PME_Status (RW1C)
        let mut cap = (crate::pci::read32(self.bdf, 0x34) & 0xFC) as u8;
        for _ in 0..16 {
            if cap == 0 {
                break;
            }
            let h = crate::pci::read32(self.bdf, cap);
            if h & 0xFF == 0x01 {
                let pmcsr = crate::pci::read32(self.bdf, cap + 4);
                crate::pci::write32(self.bdf, cap + 4, (pmcsr & !0x3) | (1 << 8) | (1 << 15)); // D0, PME_En, clear status
                return Ok(cap + 4);
            }
            cap = ((h >> 8) & 0xFC) as u8;
        }
        Err("no PCI power-management capability")
    }

    /// Stop the chip and its DMA before control leaves this kernel (chain load), following rtl8169_cleanup: close RX
    /// acceptance, StopReq (810x/8105e), then a chip reset WITH its completion wait (outstanding descriptor DMA ends with the
    /// reset), then bus mastering off. Err = the reset did not complete: the caller must not reuse that memory.
    pub fn quiesce(&self) -> Result<(), &'static str> {
        unsafe {
            let rc = self.bus.r32(self.regs.rx_config);
            self.bus.w32(self.regs.rx_config, rc & !0x3F); // rtl_rx_close: accept nothing
            if self.regs.device == 0x8136 {
                // rtl8169_cleanup default branch (covers VER_07..10, 29, 30): ChipCmd |= StopReq, 100 us
                self.bus.w8(self.regs.chip_cmd, self.bus.r8(self.regs.chip_cmd) | 0x80);
                time::delay_us(100);
            } else {
                self.bus.w8(self.regs.chip_cmd, 0); // 8139C+: no StopReq bit
                time::delay_us(10_000);
            }
        }
        let r = self.reset(); // rtl_hw_reset: CmdReset, wait for it to clear
        set_pci_cmd(self.bdf, 0, PCI_CMD_BM);
        r
    }

    /// rtl_ephy_write / rtl_ephy_read (r8169_main.c:1261-1277): EPHYAR 0x80, flag bit 31, reg 20:16, data 15:0.
    fn ephy_write(&self, reg: u32, val: u16) -> Result<(), &'static str> {
        unsafe { self.bus.w32(0x80, 0x8000_0000 | val as u32 | (reg & 0x1F) << 16) };
        for _ in 0..100 {
            time::delay_us(10);
            if unsafe { self.bus.r32(0x80) } & 0x8000_0000 == 0 {
                time::delay_us(10);
                return Ok(());
            }
        }
        Err("EPHYAR write timeout")
    }

    fn ephy_read(&self, reg: u32) -> Result<u16, &'static str> {
        unsafe { self.bus.w32(0x80, (reg & 0x1F) << 16) };
        for _ in 0..100 {
            time::delay_us(10);
            let v = unsafe { self.bus.r32(0x80) };
            if v & 0x8000_0000 != 0 {
                return Ok(v as u16);
            }
        }
        Err("EPHYAR read timeout")
    }

    /// rtl_hw_start_8105e_1 / _2 (r8169_main.c:3619-3650), VER_29/30 only. Called with Cfg9346 unlocked.
    fn hw_start_8105e(&self) -> Result<(), &'static str> {
        const E: [(u32, u16, u16); 8] =
            [(0x07, 0, 0x4000), (0x19, 0, 0x0200), (0x19, 0, 0x0020), (0x1e, 0, 0x2000), (0x03, 0, 0x0001), (0x19, 0, 0x0100), (0x19, 0, 0x0004), (0x0a, 0, 0x0020)];
        unsafe {
            let fe = self.bus.r32(0xF0); // FuncEvent: force LAN exit from ASPM if Rx/Tx not idle; no early tally
            self.bus.w32(0xF0, (fe | 0x0000_2800) & !0x0001_0000);
            self.bus.w8(0xD3, self.bus.r8(0xD3) | (1 << 3) | (1 << 2)); // MCU: EN_NDP | EN_OOB_RESET
            self.bus.w8(0xD0, self.bus.r8(0xD0) | (1 << 6)); // DLLPR: PFM_EN
        }
        for (reg, mask, bits) in E {
            let w = (self.ephy_read(reg)? & !mask) | bits;
            self.ephy_write(reg, w)?;
        }
        unsafe { self.bus.w8(0x54, self.bus.r8(0x54) & !(1 << 1)) }; // Config3: clear Rdy_to_L23 (l2l3 disable)
        if self.ver == 30 {
            let w = self.ephy_read(0x1e)? | 0x8000;
            self.ephy_write(0x1e, w)?;
        }
        kprintln!("nic: rtl_hw_start_8105e_{} applied (FuncEvent, MCU, DLLPR, 8 ephy writes, L2/L3 off)", self.ver - 28);
        Ok(())
    }

    fn reset(&self) -> Result<(), &'static str> {
        let r = self.regs;
        unsafe {
            self.bus.w8(r.chip_cmd, r.cmd_reset);
            for _ in 0..1000 {
                if self.bus.r8(r.chip_cmd) & r.cmd_reset == 0 {
                    return Ok(());
                }
                time::delay_us(100);
            }
        }
        Err("chip reset did not complete in 100 ms")
    }

    fn program_rings(&self) {
        let r = self.regs;
        unsafe {
            for i in 0..NRX {
                let d = addr_of_mut!(RX_DESC.0[i]);
                let mut o = r.desc_own | BUFSZ as u32;
                if i == NRX - 1 {
                    o |= r.ring_end;
                }
                write_volatile(d, Desc { opts1: o, opts2: 0, addr: addr_of!(RX_BUF.0[i]) as u64 });
            }
            write_volatile(addr_of_mut!(TX_DESC.0[0]), Desc { opts1: r.ring_end, opts2: 0, addr: 0 });
            fence(Ordering::SeqCst);
            let tx = addr_of!(TX_DESC) as u64;
            let rx = addr_of!(RX_DESC) as u64;
            // high dword first (r8169_main.c:2758-2767 "magic spell")
            self.bus.w32(r.tx_ring + 4, (tx >> 32) as u32);
            self.bus.w32(r.tx_ring, tx as u32);
            self.bus.w32(r.rx_ring + 4, (rx >> 32) as u32);
            self.bus.w32(r.rx_ring, rx as u32);
        }
    }

    /// Reset and bring up the NIC. Every Err path leaves bus mastering OFF.
    pub fn init(&mut self) -> Result<(), &'static str> {
        if self.regs.device == 0x8136 {
            self.init_8101e()
        } else {
            self.init_8139cp()
        }
    }

    fn init_8139cp(&mut self) -> Result<(), &'static str> {
        let r = self.regs;
        self.reset()?;
        unsafe {
            self.bus.w16(r.cplus_cmd, 0x0003); // C+ mode: CpTxOn | CpRxOn (8139cp only)
            self.program_rings();
            self.bus.w16(r.rx_max_size, BUFSZ as u16);
            self.bus.w16(r.intr_mask, 0); // polled
        }
        set_pci_cmd(self.bdf, PCI_CMD_BM, 0);
        unsafe {
            self.bus.w8(r.chip_cmd, r.cmd_rx_enb | r.cmd_tx_enb);
            self.bus.w32(r.tx_config, 0x0300_0700); // IFG normal, max DMA burst
            self.bus.w32(r.rx_config, self.rx_cfg); // accept broadcast + my phys, unlimited DMA burst
        }
        Ok(())
    }

    /// rtl_hw_start (r8169_main.c:3930-3961) reduced to the generic part, plus a PHY reset like iPXE's
    /// realtek_phy_reset. NOT done (documented gap): the per-version hw_config tables (rtl_hw_start_8102e_*,
    /// r8169_main.c:3562-3603: Config1/Config3/ephy writes) and rtl8102e_hw_phy_config. VER_10 has none.
    fn init_8101e(&mut self) -> Result<(), &'static str> {
        let r = self.regs;
        // Boot time: a PHY that already has link (it can keep it across a reset) skips the BMCR reset and its ~1.9 s
        // autonegotiation; if link is not back within 1 s, fall back to the reset. cmdline `phyreset` forces it.
        let keep = unsafe { self.bus.r8(r.phystatus) } & R::LINK_STATUS != 0 && unsafe { !FORCE_PHY_RESET };
        self.reset()?;
        unsafe {
            self.bus.w8(r.cfg9346, R::CFG9346_UNLOCK);
            let cp = self.bus.r16(r.cplus_cmd) & R::CPCMD_MASK & !(R::CPCMD_RXVLAN | R::CPCMD_RXCHKSUM);
            self.bus.w16(r.cplus_cmd, cp);
            self.bus.w8(r.max_tx_packet_size, R::TX_PACKET_MAX); // rtl_hw_start_8168 (r8169_main.c:3903)
            self.bus.w16(r.intr_mitigate, 0);
            self.bus.w16(r.rx_max_size, BUFSZ as u16); // Linux uses 16 KiB buffers; ours are 1536 B
            self.program_rings();
        }
        if self.ver == 29 || self.ver == 30 {
            if let Err(e) = self.hw_start_8105e() {
                unsafe { self.bus.w8(r.cfg9346, R::CFG9346_LOCK) };
                return Err(e); // bus mastering is still off here
            }
        }
        unsafe {
            self.bus.w8(r.cfg9346, R::CFG9346_LOCK);
            self.bus.w16(r.intr_mask, 0);
            self.bus.w16(r.intr_status, 0xFFFF);
        }
        if keep {
            kprintln!("nic: link already up at init: PHY reset skipped");
        } else {
            self.phy_reset()?;
        }
        set_pci_cmd(self.bdf, PCI_CMD_BM, 0);
        unsafe {
            self.bus.w8(r.chip_cmd, r.cmd_tx_enb | r.cmd_rx_enb); // r8169_main.c:3957, before Rx/TxConfig
            self.bus.w32(r.rx_config, self.rx_cfg);
            self.bus.w32(r.tx_config, R::TX_DMA_BURST << 8 | R::INTER_FRAME_GAP << 24);
        }
        // No waiting here: autonegotiation (~2 s) runs in the PHY while the home screen comes up; bmctl::run's
        // poll starts DHCP when the link appears, and net.rs sends nothing while it is down.
        let st = unsafe { self.bus.r8(r.phystatus) };
        if keep && st & R::LINK_STATUS == 0 {
            kprintln!("nic: link lost at the chip reset: resetting the PHY after all");
            self.phy_reset()?;
        }
        kprintln!("nic: PHYstatus {:#04x} link {} at init", st, if st & R::LINK_STATUS != 0 { "up" } else { "not up yet" });
        Ok(())
    }

    pub fn link_up(&self) -> bool {
        link_up(self.bus, self.regs)
    }

    /// Non-blocking: if the next RX descriptor is ours, return (index, frame length without FCS).
    /// The caller must hand the descriptor back with `rx_release(index)`.
    pub fn rx_poll(&mut self) -> Option<(usize, usize)> {
        let r = self.regs;
        let i = self.rx_next;
        unsafe {
            let o = read_volatile(addr_of!(RX_DESC.0[i].opts1));
            if o & r.desc_own != 0 {
                return None;
            }
            fence(Ordering::SeqCst);
            self.rx_next = (i + 1) % NRX;
            if o & (r.rx_err | r.first_frag | r.last_frag) != (r.first_frag | r.last_frag) {
                rx_release(r, i); // errored or multi-descriptor frame: drop it
                return None;
            }
            Some((i, ((o & r.rx_len_mask) as usize).saturating_sub(4).min(BUFSZ)))
        }
    }
}

/// Only valid between `rx_poll` returning (i, len) and `rx_release(i)` (net::Rx enforces this).
pub(crate) fn rx_buf(i: usize, len: usize) -> &'static [u8] {
    assert!(i < NRX && len <= BUFSZ);
    unsafe { core::slice::from_raw_parts(addr_of!(RX_BUF.0[i]) as *const u8, len) }
}

pub fn rx_release(r: &Regs, i: usize) {
    let mut no = r.desc_own | BUFSZ as u32;
    if i == NRX - 1 {
        no |= r.ring_end;
    }
    fence(Ordering::SeqCst);
    unsafe { write_volatile(addr_of_mut!(RX_DESC.0[i].opts1), no) };
}

/// Cable present (8139cp has no PHYstatus register: assume up).
pub fn link_up(bus: Bus, r: &Regs) -> bool {
    r.phystatus == 0 || unsafe { bus.r8(r.phystatus) } & R::LINK_STATUS != 0
}

/// After a link comes back: re-enable TX if the NIC has meanwhile released the descriptor it was holding.
pub fn tx_recover(r: &Regs) {
    unsafe {
        if TX_WEDGED && read_volatile(addr_of!(TX_DESC.0[0].opts1)) & r.desc_own == 0 {
            TX_WEDGED = false;
            kprintln!("nic: TX descriptor back, TX re-enabled");
        }
    }
}

/// Set once a TX descriptor fails to come back (OWN still set after the deadline). From then on the
/// TX DMA buffer and descriptor are treated as owned by the NIC forever: nothing is written to them.
static mut TX_WEDGED: bool = false;
static mut TX_SCRATCH: [u8; BUFSZ] = [0; BUFSZ];

/// Let `fill` build a `len`-byte frame directly in the DMA buffer, then send it.
/// Never touches the buffer/descriptor unless the NIC has cleared OWN (Codex M2 MUST-FIX).
pub fn tx_with<R2>(bus: Bus, r: &Regs, len: usize, fill: impl FnOnce(&mut [u8]) -> R2) -> R2 {
    let len = len.min(BUFSZ);
    unsafe {
        let d = addr_of_mut!(TX_DESC.0[0]);
        if !link_up(bus, r) {
            // no link: never hand the NIC a frame it may keep forever; dropped, and TX stays usable
            let scratch = &mut *core::ptr::addr_of_mut!(TX_SCRATCH);
            return fill(&mut scratch[..len]);
        }
        if TX_WEDGED || read_volatile(addr_of!((*d).opts1)) & r.desc_own != 0 {
            TX_WEDGED = true;
            // Frame is built into scratch and dropped: the NIC may still be DMAing the real buffer.
            let scratch = &mut *core::ptr::addr_of_mut!(TX_SCRATCH);
            return fill(&mut scratch[..len]);
        }
        let buf = core::slice::from_raw_parts_mut(addr_of_mut!(TX_BUF.0[0]) as *mut u8, BUFSZ);
        let res = fill(&mut buf[..len]);
        let wire = len.max(60);
        for b in &mut buf[len..wire] {
            *b = 0;
        }
        write_volatile(
            d,
            Desc {
                opts1: r.desc_own | r.ring_end | r.first_frag | r.last_frag | wire as u32,
                opts2: 0,
                addr: buf.as_ptr() as u64,
            },
        );
        fence(Ordering::SeqCst);
        bus.w8(r.tx_poll, r.npq);
        let t0 = rdtsc();
        while read_volatile(addr_of!((*d).opts1)) & r.desc_own != 0 {
            if rdtsc() - t0 > 2_000_000_000 {
                TX_WEDGED = true;
                kprintln!("nic: TX descriptor not returned by NIC, TX disabled");
                break;
            }
            core::hint::spin_loop();
        }
        fence(Ordering::SeqCst);
        let st = bus.r16(r.intr_status);
        bus.w16(r.intr_status, st);
        res
    }
}
