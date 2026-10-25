//! Intel Centrino Wireless-N 130 (PCI 8086:0896, iwlwifi DVM family, firmware iwlwifi-6000g2b-6.ucode) brought up
//! by polling, step by step (bmctl `wifi ...`). Nothing runs at boot: every step is an explicit command, so a bad
//! step is seen while someone watches. Recipe: docs/iwl-dvm-bringup.md (from the Linux 6.18 driver).
//! Step 1 here: probe, NIC access, firmware TLV parsing.
use crate::pci::{self, Bdf};
use core::fmt::Write;

/// The firmware file, embedded (fw/ is local only: Intel's redistributable linux-firmware blob, not in git).
static FW: &[u8] = include_bytes!("../fw/iwlwifi-6000g2b-6.ucode");
// A saved error page or a compressed copy would otherwise build fine and only fail on the netbook.
const _: () = assert!(FW.len() > 88 && FW[4] == b'I' && FW[5] == b'W' && FW[6] == b'L' && FW[7] == b'\n',
    "fw/iwlwifi-6000g2b-6.ucode is not iwlwifi firmware: download it again and check its SHA-256 (README)");

pub const CSR_HW_IF_CONFIG: u32 = 0x000;
pub const CSR_INT: u32 = 0x008;
pub const CSR_INT_MASK: u32 = 0x00C;
pub const CSR_FH_INT_STATUS: u32 = 0x010;
pub const CSR_RESET: u32 = 0x020;
pub const CSR_GP_CNTRL: u32 = 0x024;
pub const CSR_HW_REV: u32 = 0x028;
pub const HBUS_PRPH_WADDR: u32 = 0x444;
pub const HBUS_PRPH_RADDR: u32 = 0x448;
pub const HBUS_PRPH_WDAT: u32 = 0x44C;
pub const HBUS_PRPH_RDAT: u32 = 0x450;
pub const GP_MAC_CLOCK_READY: u32 = 0x1;
pub const GP_INIT_DONE: u32 = 0x4;
pub const GP_MAC_ACCESS_REQ: u32 = 0x8;
pub const GP_GOING_TO_SLEEP: u32 = 0x10;
pub const GP_HW_RF_KILL_SW: u32 = 0x0800_0000;

pub struct Iwl {
    pub bdf: Bdf,
    pub bar: u64,
}

impl Iwl {
    pub fn rd(&self, off: u32) -> u32 {
        unsafe { core::ptr::read_volatile((self.bar + off as u64) as *const u32) }
    }
    pub fn wr(&self, off: u32, v: u32) {
        unsafe { core::ptr::write_volatile((self.bar + off as u64) as *mut u32, v) }
    }
    pub fn set(&self, off: u32, bits: u32) {
        self.wr(off, self.rd(off) | bits);
    }
    pub fn clear(&self, off: u32, bits: u32) {
        self.wr(off, self.rd(off) & !bits);
    }
    /// Poll `off` until (value & mask) == want, at most `us` microseconds.
    pub fn poll(&self, off: u32, mask: u32, want: u32, us: u32) -> bool {
        for _ in 0..us / 10 + 1 {
            if self.rd(off) & mask == want {
                return true;
            }
            crate::time::delay_us(10);
        }
        false
    }
    /// Wake the MAC for register/SRAM access (iwl_trans_pcie_grab_nic_access).
    pub fn grab(&self) -> bool {
        self.set(CSR_GP_CNTRL, GP_MAC_ACCESS_REQ);
        self.poll(CSR_GP_CNTRL, GP_MAC_CLOCK_READY | GP_GOING_TO_SLEEP, GP_MAC_CLOCK_READY, 15_000)
    }
    pub fn release(&self) {
        self.clear(CSR_GP_CNTRL, GP_MAC_ACCESS_REQ);
    }
    pub fn prph_rd(&self, a: u32) -> u32 {
        self.wr(HBUS_PRPH_RADDR, (a & 0x000F_FFFF) | (3 << 24));
        self.rd(HBUS_PRPH_RDAT)
    }
    pub fn prph_wr(&self, a: u32, v: u32) {
        self.wr(HBUS_PRPH_WADDR, (a & 0x000F_FFFF) | (3 << 24));
        self.wr(HBUS_PRPH_WDAT, v);
    }
}

impl Iwl {
    pub fn prph_set(&self, a: u32, bits: u32) {
        let v = self.prph_rd(a);
        self.prph_wr(a, v | bits);
    }

    /// iwl_pcie_apm_init for the 6000 family (pcie/gen1_2/trans.c:231): L0s off, HAP wake, INIT_DONE -> MAC clock,
    /// DMA clock on, L1-Active off, RF-kill interrupt cleared, power source VMAIN (iwl_pcie_set_pwr).
    pub fn apm_init(&self) -> Result<(), &'static str> {
        self.set(0x100, 0x2000_0000); // CSR_GIO_CHICKEN_BITS: DIS_L0S_EXIT_TIMER
        self.set(0x100, 0x0080_0000); // L1A_NO_L0S_RX
        self.set(0x240, 0xFFFF_0000); // CSR_DBG_HPET_MEM_REG: FH wait threshold max
        self.set(CSR_HW_IF_CONFIG, 0x0008_0000); // HAP_WAKE
        self.set(0x03C, 0x2); // CSR_GIO_REG: L0S_DISABLED
        self.set(CSR_GP_CNTRL, GP_INIT_DONE);
        if !self.poll(CSR_GP_CNTRL, GP_MAC_CLOCK_READY, GP_MAC_CLOCK_READY, 25_000) {
            return Err("MAC clock not ready after INIT_DONE");
        }
        if !self.grab() {
            return Err("no NIC access after APM");
        }
        self.prph_wr(0x3004, 0x200); // APMG_CLK_EN_REG: DMA_CLK_RQT
        crate::time::delay_us(20);
        self.prph_set(0x3010, 0x800); // APMG_PCIDEV_STT_REG: L1_ACT_DIS
        self.prph_wr(0x301C, 0x1000_0000); // APMG_RTC_INT_STT_REG: clear RFKILL
        let ps = self.prph_rd(0x300C) & !0x0300_0000; // APMG_PS_CTRL_REG: power source VMAIN
        self.prph_wr(0x300C, ps);
        self.release();
        Ok(())
    }
}

/// The card's NVM (2 KiB OTP image on the 6000 family, shadow RAM, so no link-list walk): dvm/eeprom.c:918.
pub static mut NVM: [u8; 2048] = [0; 2048];

impl Iwl {
    pub fn read_nvm(&self) -> Result<bool, &'static str> {
        let otp = self.rd(0x034) & 0x0001_0000 != 0; // CSR_OTP_GP_REG.DEVICE_SELECT
        let sig = self.rd(0x030) & 7; // CSR_EEPROM_GP signature
        if !(otp && sig == 1 || !otp && (sig == 2 || sig == 4)) {
            return Err("bad EEPROM/OTP signature");
        }
        let mut got = false;
        for _ in 0..1000 {
            self.set(CSR_HW_IF_CONFIG, 0x0020_0000); // EEPROM_OWN_SEM
            if self.poll(CSR_HW_IF_CONFIG, 0x0020_0000, 0x0020_0000, 10) {
                got = true;
                break;
            }
        }
        if !got {
            return Err("EEPROM semaphore");
        }
        let r = (|| {
            if otp {
                self.set(CSR_GP_CNTRL, GP_INIT_DONE);
                if !self.poll(CSR_GP_CNTRL, GP_MAC_CLOCK_READY, GP_MAC_CLOCK_READY, 25_000) || !self.grab() {
                    return Err("OTP: no MAC clock");
                }
                self.prph_set(0x300C, 0x0400_0000); // APMG_PS_CTRL: RESET_REQ pulse
                crate::time::delay_us(5);
                let v = self.prph_rd(0x300C);
                self.prph_wr(0x300C, v & !0x0400_0000);
                self.release();
                self.set(0x250, 0x8000_0000); // CSR_DBG_LINK_PWR_MGMT: shadow RAM, link power mgmt off
                self.wr(0x030, self.rd(0x030) & !0x180); // EEPROM_GP: owner = driver
                self.set(0x034, 0x0030_0000); // ack ECC status bits
            }
            let nvm = unsafe { &mut *core::ptr::addr_of_mut!(NVM) };
            for a in (0..2048u32).step_by(2) {
                self.wr(0x02C, a << 1 & 0xFFFC); // CSR_EEPROM_REG address
                if !self.poll(0x02C, 1, 1, 5000) {
                    return Err("NVM read timeout");
                }
                let d = (self.rd(0x02C) >> 16) as u16;
                if otp && self.rd(0x034) & 0x0020_0000 != 0 {
                    return Err("OTP uncorrectable ECC error");
                }
                nvm[a as usize..a as usize + 2].copy_from_slice(&d.to_le_bytes());
            }
            Ok(otp)
        })();
        self.clear(CSR_HW_IF_CONFIG, 0x0020_0000);
        r
    }
}

// DMA memory (identity-mapped statics below 4 GiB): 64 receive buffers of 4 KiB behind a 256-entry RBD ring, the RB
// status area, the keep-warm page, and one 128 KiB bounce buffer for firmware chunks (FH_MEM_TB_MAX_LENGTH).
pub(crate) const NRB: usize = 64;
#[repr(C, align(256))]
pub(crate) struct Rbd(pub(crate) [u32; 256]);
#[repr(C, align(4096))]
pub(crate) struct Rbs(pub(crate) [[u8; 4096]; NRB]);
#[repr(C, align(16))]
pub(crate) struct RbStatus(pub(crate) [u16; 8]);
#[repr(C, align(4096))]
pub(crate) struct Page(pub(crate) [u8; 4096]);
#[repr(C, align(4096))]
struct Chunk([u8; 0x20000]);
pub(crate) static mut RBD: Rbd = Rbd([0; 256]);
pub(crate) static mut RBS: Rbs = Rbs([[0; 4096]; NRB]);
pub(crate) static mut RB_STATUS: RbStatus = RbStatus([0; 8]);
pub(crate) static mut KW: Page = Page([0; 4096]);
static mut CHUNK: Chunk = Chunk([0; 0x20000]);
pub(crate) static mut RX_READ: usize = 0; // next RB index to consume (0..256)

pub(crate) fn pa<T>(p: *const T) -> u32 {
    p as u64 as u32
}

impl Iwl {
    /// iwl_nic_config (dvm/main.c:1997) for the 6030 family: MAC step/dash from HW_REV, radio type/step/dash from
    /// the NVM radio config, RADIO_SI | MAC_SI, early power-off reset disabled. No GP_DRIVER bits for 6030.
    pub fn nic_config(&self) {
        let n = unsafe { &*core::ptr::addr_of!(NVM) };
        let rc = u16::from_le_bytes([n[0x90], n[0x91]]) as u32;
        let v = self.rd(CSR_HW_IF_CONFIG) & !0xFF0F;
        let v = v | (self.rd(CSR_HW_REV) & 0xF) | (rc & 3) << 10 | (rc >> 4 & 3) << 12 | (rc >> 2 & 3) << 14 | 0x300;
        self.wr(CSR_HW_IF_CONFIG, v);
        if self.grab() {
            self.prph_set(0x300C, 0x0040_0000);
            self.release();
        }
    }

    /// iwl_pcie_rx_hw_init (pcie/gen1_2/rx.c:836): 256-entry RBD ring, 4 KiB RBs, NRB of them filled.
    pub fn rx_init(&self) -> bool {
        unsafe {
            let rbd = &mut *core::ptr::addr_of_mut!(RBD);
            let rbs = &*core::ptr::addr_of!(RBS);
            for i in 0..256 {
                rbd.0[i] = pa(&rbs.0[i % NRB]) >> 8;
            }
            *core::ptr::addr_of_mut!(RB_STATUS) = RbStatus([0; 8]);
            RX_READ = 0;
        }
        if !self.grab() {
            return false;
        }
        self.wr(0x1C00, 0); // FH_MEM_RCSR_CHNL0_CONFIG_REG: stop
        self.wr(0x1C08, 0); // RBDCB_WPTR
        self.wr(0x1C10, 0); // FLUSH_RB_REQ
        self.wr(0x1BCC, 0); // RDPTR
        self.wr(0x1BC8, 0); // RBDCB_WPTR_REG
        self.wr(0x1BC4, pa(core::ptr::addr_of!(RBD)) >> 8); // RBDCB_BASE
        self.wr(0x1BC0, pa(core::ptr::addr_of!(RB_STATUS)) >> 4); // STTS_WPTR
        self.wr(0x1C00, 0x8000_0000 | 0x4 | 0x1000 | 0x11 << 4 | 8 << 20); // enable, ignore RXF empty, IRQ to host, 4K, timeout, 256 RBDs
        self.release();
        self.wr(0x004, 0x40); // CSR_INT_COALESCING default
        self.rx_kick(NRB as u32);
        true
    }

    /// Give the device RBs up to ring index `w` (a multiple of 8).
    pub(crate) fn rx_kick(&self, w: u32) {
        self.wr(0x1BC8, w & !7 & 0xFF);
    }

    /// Next received packet, if the device closed one: (cmd id, sequence, payload).
    pub fn rx_next(&self) -> Option<(u8, u16, &'static [u8])> {
        let closed = unsafe { (*core::ptr::addr_of!(RB_STATUS)).0[0] } as usize & 0xFFF;
        let r = unsafe { RX_READ };
        if closed % 256 == r {
            return None;
        }
        let rb = unsafe { &(*core::ptr::addr_of!(RBS)).0[r % NRB] };
        unsafe { RX_READ = (r + 1) % 256 };
        let len = (u32::from_le_bytes([rb[0], rb[1], rb[2], rb[3]]) & 0x3FFF) as usize;
        let seq = u16::from_le_bytes([rb[6], rb[7]]);
        // hand the slot NRB-8 behind us back (whole groups of 8)
        if (r + 1) % 8 == 0 {
            self.rx_kick(((r + 1 + NRB - 8) % 256) as u32);
        }
        Some((rb[4], seq, &rb[8..(4 + len).min(4096).max(8)]))
    }

    /// One firmware section into SRAM over the FH service channel (pcie/gen1_2/trans.c:600-680), polled.
    pub(crate) fn load_section(&self, dst: u32, data: &[u8]) -> Result<(), &'static str> {
        for (i, part) in data.chunks(0x20000).enumerate() {
            let buf = unsafe { &mut (*core::ptr::addr_of_mut!(CHUNK)).0 };
            buf[..part.len()].copy_from_slice(part);
            self.wr(CSR_INT, 1 << 27);
            self.wr(CSR_FH_INT_STATUS, 0x3);
            if !self.grab() {
                return Err("no NIC access for firmware load");
            }
            self.wr(0x1E20, 0); // FH_TCSR_CHNL_TX_CONFIG_REG(9): pause
            self.wr(0x19C8, dst + (i * 0x20000) as u32); // FH_SRVC_CHNL_SRAM_ADDR_REG(9)
            self.wr(0x1948, pa(core::ptr::addr_of!(CHUNK))); // FH_TFDIB_CTRL0_REG(9)
            self.wr(0x194C, part.len() as u32); // FH_TFDIB_CTRL1_REG(9): hi addr 0
            self.wr(0x1E28, 1 << 20 | 1 << 12 | 3); // TX_BUF_STS: one TB, valid
            self.wr(0x1E20, 0x8000_0000 | 0x0010_0000); // enable, credit off, IRQ at end of TFD
            self.release();
            if !self.poll(CSR_INT, 1 << 27, 1 << 27, 2_000_000) {
                return Err("firmware chunk DMA timed out");
            }
            self.wr(CSR_INT, 1 << 27);
            self.wr(CSR_FH_INT_STATUS, 0x3);
        }
        Ok(())
    }

    /// Load an image (init: TLVs 3/4, runtime: 1/2), release the CPU and wait for REPLY_ALIVE.
    pub fn boot(&self, init: bool, w: &mut dyn Write) -> Result<(), &'static str> {
        let (ti, td) = if init { (3, 4) } else { (1, 2) };
        let inst = tlvs().find(|t| t.0 == ti).ok_or("no inst TLV")?.1;
        let data = tlvs().find(|t| t.0 == td).ok_or("no data TLV")?.1;
        self.wr(CSR_INT, 0xFFFF_FFFF);
        self.wr(CSR_FH_INT_STATUS, 0xFFFF_FFFF);
        self.wr(0x05C, 0x2); // CSR_UCODE_DRV_GP1_CLR: SW RF-kill
        self.wr(0x05C, 0x4); // CMD_BLOCKED
        self.load_section(0x0, inst)?;
        self.load_section(0x80_0000, data)?;
        let _ = write!(w, "loaded {} + {} B; ", inst.len(), data.len());
        self.wr(CSR_RESET, 0); // release the embedded CPU
        for _ in 0..2000 {
            while let Some((cmd, _seq, p)) = self.rx_next() {
                if cmd == 1 && p.len() >= 32 {
                    let valid = u32::from_le_bytes([p[28], p[29], p[30], p[31]]);
                    let _ = write!(w, "ALIVE: ucode {}.{} subtype {} valid {} err_table {:#x}", p[1], p[0], p[13], valid,
                        u32::from_le_bytes([p[20], p[21], p[22], p[23]]));
                    return if valid == 1 { Ok(()) } else { Err("ALIVE but not valid") };
                }
                let _ = write!(w, "rx cmd {:#x} len {}; ", cmd, p.len());
            }
            crate::wdt::pet();
            crate::wdt::delay_ms(1);
        }
        let _ = write!(w, "csr_int {:#x} fh_int {:#x} gp1 {:#x} rb_status {:?}; ", self.rd(CSR_INT), self.rd(CSR_FH_INT_STATUS), self.rd(0x054),
            unsafe { (*core::ptr::addr_of!(RB_STATUS)).0 });
        Err("no ALIVE within 2 s")
    }
}

pub fn probe() -> Result<Iwl, &'static str> {
    let bdf = pci::find(0x8086, 0x0896).ok_or("no 8086:0896")?;
    // D0 (a `pwr d3` may have parked it), memory + bus master on
    let mut cap = (pci::read32(bdf, 0x34) & 0xFC) as u8;
    for _ in 0..16 {
        if cap == 0 {
            break;
        }
        let h = pci::read32(bdf, cap);
        if h & 0xFF == 1 {
            let cs = pci::read32(bdf, cap + 4);
            if cs & 3 != 0 {
                pci::write32(bdf, cap + 4, cs & !3);
                crate::wdt::delay_ms(10);
            }
            break;
        }
        cap = (h >> 8 & 0xFC) as u8;
    }
    pci::write32(bdf, 0x04, pci::read32(bdf, 0x04) | 0x6);
    if pci::read32(bdf, 0x10) & !0xF == 0 {
        // D3hot -> D0 lost the BAR (a `pwr d3` before a chain-load): put it back at the base of the upstream
        // bridge's memory window, where the BIOS had it
        for f in 0..8 {
            let br = pci::Bdf { bus: 0, dev: 0x1c, func: f };
            if pci::read32(br, 0x18) >> 8 & 0xFF == bdf.bus as u32 {
                let w = pci::read32(br, 0x20);
                let (base, limit) = ((w & 0xFFF0) << 16, (w >> 16 & 0xFFF0) << 16 | 0xF_FFFF);
                if base == 0 || base > limit || limit - base < 0x1FFF {
                    break; // window disabled or too small: leave BAR0 at 0 (probe fails cleanly)
                }
                pci::write32(bdf, 0x10, base);
                pci::write32(bdf, 0x14, 0);
                crate::kprintln!("wifi: BAR0 was 0 (after D3): restored to {:#x} from bridge 00:1c.{}", base, f);
                break;
            }
        }
    }
    let bar = (pci::read32(bdf, 0x10) & !0xF) as u64;
    if bar == 0 || !crate::nic::map_uc_4k(bar) || !crate::nic::map_uc_4k(bar + 0x1000) {
        return Err("BAR0 not mappable");
    }
    // PCI_CFG_RETRY_TIMEOUT (0x41) = 0, as iwl_pci_probe does
    let r = pci::read32(bdf, 0x40);
    pci::write32(bdf, 0x40, r & !0xFF00);
    Ok(Iwl { bdf, bar })
}

/// (type, payload) of each TLV after the 88-byte header.
pub fn tlvs() -> impl Iterator<Item = (u32, &'static [u8])> {
    let mut i = 88usize;
    core::iter::from_fn(move || {
        if i + 8 > FW.len() {
            return None;
        }
        let t = u16::from_le_bytes([FW[i], FW[i + 1]]) as u32;
        let l = u32::from_le_bytes([FW[i + 4], FW[i + 5], FW[i + 6], FW[i + 7]]) as usize;
        let p = FW.get(i + 8..i + 8 + l)?;
        i = (i + 8 + l + 3) & !3;
        Some((t, p))
    })
}

pub fn cmd<'a>(w: &mut dyn Write, it: &mut impl Iterator<Item = &'a str>) {
    match it.next().unwrap_or("") {
        "fw" => {
            let magic = u32::from_le_bytes([FW[4], FW[5], FW[6], FW[7]]);
            let name = core::str::from_utf8(&FW[8..72]).unwrap_or("?").trim_end_matches('\0').trim_end();
            let _ = write!(w, "fw {} B magic {:#x} '{}':", FW.len(), magic, name);
            for (t, p) in tlvs() {
                let _ = write!(w, " {}:{}", t, p.len());
            }
        }
        "probe" => match probe() {
            Ok(d) => {
                let got = d.grab();
                let _ = write!(w, "{} bar {:#x} hw_rev {:#x} gp_cntrl {:#x} (rfkill switch {}) hw_if {:#x} int {:#x} reset {:#x} nic access {}",
                    d.bdf, d.bar, d.rd(CSR_HW_REV), d.rd(CSR_GP_CNTRL), if d.rd(CSR_GP_CNTRL) & GP_HW_RF_KILL_SW != 0 { "on" } else { "OFF" },
                    d.rd(CSR_HW_IF_CONFIG), d.rd(CSR_INT), d.rd(CSR_RESET), got);
                d.release();
            }
            Err(e) => {
                let _ = write!(w, "err {}", e);
            }
        },
        "apm" => match probe().and_then(|d| d.apm_init().map(|_| d)) {
            Ok(d) => {
                let got = d.grab();
                let clk = d.prph_rd(0x3000); // APMG_CLK_CTRL_REG
                let _ = write!(w, "apm ok: gp_cntrl {:#x} nic access {} apmg_clk_ctrl {:#x} ps_ctrl {:#x} hw_if {:#x}", d.rd(CSR_GP_CNTRL), got, clk, d.prph_rd(0x300C), d.rd(CSR_HW_IF_CONFIG));
                d.release();
            }
            Err(e) => {
                let _ = write!(w, "err {}", e);
            }
        },
        "nvm" => match probe().and_then(|d| d.apm_init().map(|_| d)).and_then(|d| d.read_nvm()) {
            Ok(otp) => {
                let n = unsafe { &*core::ptr::addr_of!(NVM) };
                let m = &n[0x2A..0x30];
                let w16 = |o: usize| u16::from_le_bytes([n[o], n[o + 1]]);
                let _ = write!(w, "{} ok: MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} version {:#x} sku {:#x} radio_cfg {:#x}",
                    if otp { "OTP" } else { "EEPROM" }, m[0], m[1], m[2], m[3], m[4], m[5], w16(0x88), w16(0x8A), w16(0x90));
            }
            Err(e) => {
                let _ = write!(w, "err {}", e);
            }
        },
        s @ ("init" | "rt") => {
            let r = probe().and_then(|d| {
                d.apm_init()?;
                d.read_nvm()?;
                d.apm_init()?; // OTP access reset the APMG power state: bring the NIC up again
                d.nic_config();
                if !d.rx_init() {
                    return Err("rx init: no NIC access");
                }
                if d.grab() {
                    d.wr(0x197C, pa(core::ptr::addr_of!(KW)) >> 4); // FH_KW_MEM_ADDR_REG: keep-warm page
                    d.release();
                }
                d.set(0x0A8, 0x800F_FFFF); // CSR_MAC_SHADOW_REG_CTRL
                d.boot(s == "init", w)
            });
            if let Err(e) = r {
                let _ = write!(w, "err {}", e);
            }
        }
        s @ ("up" | "scan" | "list" | "join" | "dhcp" | "stats") => crate::iwl::cmd(w, s, it),
        _ => {
            let _ = write!(w, "err wifi fw|probe|apm|nvm|init|rt|up|scan|list");
        }
    }
}
