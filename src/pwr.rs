//! Power knobs for measuring what the NC110 draws (bmctl `pwr ...`). Each one only lowers power and lasts until the
//! next reset, which undoes it: the disk spins up again for GRUB, PCI devices come back in D0, the CPU at full clock.
//! `pwr pci` lists devices; `pwr msr <hex>` reads an allowlisted MSR; `pwr hdd` spins the ATA disk down (we never
//! touch it after boot); `pwr d3 <bus:dev.fn>` puts a WiFi or USB controller in D3hot; `pwr pstate <hex>` writes
//! IA32_PERF_CTL; `pwr cstate <hex>|hlt` picks the idle MWAIT hint; `pwr bl <0-100>` sets the GMA backlight duty.
use crate::pci::{self, Bdf};
use crate::port::{inb, outb};
use core::arch::asm;
use core::fmt::Write;

fn rdmsr(m: u32) -> u64 {
    let (lo, hi): (u32, u32);
    unsafe { asm!("rdmsr", in("ecx") m, out("eax") lo, out("edx") hi, options(nostack)) };
    (hi as u64) << 32 | lo as u64
}

fn cpuid1_ecx() -> u32 {
    let c: u32;
    unsafe { asm!("push rbx", "cpuid", "pop rbx", inout("eax") 1u32 => _, out("ecx") c, out("edx") _, options(nostack)) };
    c
}

fn parse_bdf(s: &str) -> Option<Bdf> {
    let (b, rest) = s.split_once(':')?;
    let (d, f) = rest.split_once('.')?;
    let (b, d, f) = (u8::from_str_radix(b, 16).ok()?, u8::from_str_radix(d, 16).ok()?, u8::from_str_radix(f, 10).ok()?);
    let mut hit = None;
    pci::walk(&mut |x, _| {
        if x.bus == b && x.dev == d && x.func == f {
            hit = Some(x);
        }
        false
    });
    hit
}

#[repr(C, align(1024))]
struct Clb([u32; 256]); // 32 command headers x 8 dwords
#[repr(C, align(256))]
struct Rfis([u8; 256]);
#[repr(C, align(128))]
struct Ctbl([u8; 256]); // command FIS at 0, no PRDs
static mut CLB: Clb = Clb([0; 256]);
static mut RFIS: Rfis = Rfis([0; 256]);
static mut CTBL: Ctbl = Ctbl([0; 256]);

/// AHCI: STANDBY IMMEDIATE (0xE0, no data) on every implemented port with a device, using our own command list and
/// FIS area (the port is stopped first, so whatever GRUB left there is never followed).
fn ahci_standby(w: &mut dyn Write, b: Bdf, go: bool) {
    let abar = (pci::read32(b, 0x24) & !0x3FF) as u64; // ABAR is 1 KiB aligned on the NM10
    if abar == 0 || !crate::nic::map_uc_4k(abar) {
        let _ = write!(w, "err AHCI ABAR {:#x} not mappable", abar);
        return;
    }
    let rd = |o: u64| unsafe { core::ptr::read_volatile((abar + o) as *const u32) };
    let wr = |o: u64, v: u32| unsafe { core::ptr::write_volatile((abar + o) as *mut u32, v) };
    let pi = rd(0x0C);
    let _ = write!(w, "bars {:#x} {:#x} {:#x} {:#x} {:#x}; ", pci::read32(b, 0x10), pci::read32(b, 0x14), pci::read32(b, 0x18), pci::read32(b, 0x1C), pci::read32(b, 0x20));
    let _ = write!(w, "abar {:#x} pcicmd {:#x} bar5 {:#x} cap {:#x} ghc {:#x} pi {:#x}; ", abar, pci::read32(b, 0x04) & 0xFFFF, pci::read32(b, 0x24), rd(0x00), rd(0x04), pi);
    for p in 0..32u64 {
        if pi & (1 << p) != 0 && pi != u32::MAX {
            let pb = 0x100 + 0x80 * p;
            let _ = write!(w, "p{} ssts {:#x} sig {:#x} cmd {:#x} tfd {:#x}; ", p, rd(pb + 0x28), rd(pb + 0x24), rd(pb + 0x18), rd(pb + 0x20));
        }
    }
    if !go {
        return;
    }
    if rd(0x04) & (1 << 31) == 0 {
        // GHC.AE clear: the BIOS runs this controller in legacy mode; the disk answers on the native IDE I/O BARs.
        let base = [(pci::read32(b, 0x10) & 0xFFFC) as u16, (pci::read32(b, 0x18) & 0xFFFC) as u16];
        let _ = write!(w, "AHCI off, IDE ports {:#x} {:#x}: ", base[0], base[1]);
        ide_standby(w, base);
        return;
    }
    let wait = |o: u64, mask: u32, want: u32| {
        for _ in 0..500 {
            if rd(o) & mask == want {
                return true;
            }
            crate::wdt::delay_ms(1);
        }
        false
    };
    for p in 0..32u64 {
        if pi & (1 << p) == 0 {
            continue;
        }
        let pb = 0x100 + 0x80 * p;
        if rd(pb + 0x28) & 0xF != 3 {
            continue; // SSTS.DET: no device with PHY up
        }
        // stop: ST off, wait CR; FRE off, wait FR
        wr(pb + 0x18, rd(pb + 0x18) & !1);
        let a = wait(pb + 0x18, 1 << 15, 0);
        wr(pb + 0x18, rd(pb + 0x18) & !(1 << 4));
        let c = wait(pb + 0x18, 1 << 14, 0);
        if !(a && c) {
            let _ = write!(w, "port {} would not stop; ", p);
            continue;
        }
        unsafe {
            let clb = &mut *core::ptr::addr_of_mut!(CLB);
            let ct = &mut *core::ptr::addr_of_mut!(CTBL);
            clb.0 = [0; 256];
            ct.0 = [0; 256];
            ct.0[0] = 0x27; // H2D register FIS
            ct.0[1] = 0x80; // command
            ct.0[2] = 0xE0; // STANDBY IMMEDIATE
            ct.0[7] = 0xA0; // device
            clb.0[0] = 5; // CFL 5 dwords, read, PRDTL 0
            clb.0[2] = core::ptr::addr_of!(CTBL) as u32;
            wr(pb + 0x00, core::ptr::addr_of!(CLB) as u32);
            wr(pb + 0x04, 0);
            wr(pb + 0x08, core::ptr::addr_of!(RFIS) as u32);
            wr(pb + 0x0C, 0);
        }
        wr(pb + 0x10, 0xFFFF_FFFF); // clear PxIS
        wr(pb + 0x30, 0xFFFF_FFFF); // clear PxSERR
        wr(pb + 0x18, rd(pb + 0x18) | 1 << 4); // FRE
        wr(pb + 0x18, rd(pb + 0x18) | 1); // ST
        wr(pb + 0x38, 1); // CI slot 0
        let mut done = false;
        for _ in 0..15 { // short: the bmctl reply must beat the client's retry window; `pwr hdd` again shows the end state
            if rd(pb + 0x38) & 1 == 0 {
                done = true;
                break;
            }
            crate::wdt::pet();
            crate::wdt::delay_ms(100);
        }
        let _ = write!(w, "port {} standby {} (TFD {:#x} IS {:#x}); ", p, if done { "ok" } else { "timeout" }, rd(pb + 0x20), rd(pb + 0x10));
    }
}

/// ATA STANDBY IMMEDIATE on both drives of both channels of an IDE-mode SATA controller (PIO, legacy or BAR ports).
fn hdd(w: &mut dyn Write, go: bool) {
    let mut ide = None;
    pci::walk(&mut |b, _| {
        if pci::read32(b, 0x08) >> 16 == 0x0101 && ide.is_none() {
            ide = Some(b);
        }
        false
    });
    let Some(b) = ide else {
        let mut ahci = None;
        pci::walk(&mut |b, _| {
            if pci::read32(b, 0x08) >> 8 == 0x010601 && ahci.is_none() {
                ahci = Some(b);
            }
            false
        });
        match ahci {
            Some(b) => ahci_standby(w, b, go),
            None => {
                let _ = write!(w, "err no IDE or AHCI disk controller");
            }
        }
        return;
    };
    let pi = (pci::read32(b, 0x08) >> 8) as u8;
    let port = |ch: u8| if pi & (1 << (2 * ch)) != 0 { (pci::read32(b, 0x10 + 8 * ch) & 0xFFFC) as u16 } else if ch == 0 { 0x1F0 } else { 0x170 };
    if go {
        ide_standby(w, [port(0), port(1)]);
    }
}

fn ide_standby(w: &mut dyn Write, bases: [u16; 2]) {
    for (ch, &base) in bases.iter().enumerate() {
        for dr in 0..2u8 {
            unsafe {
                outb(base + 6, 0xA0 | dr << 4);
                let st = inb(base + 7);
                if st == 0xFF || st == 0 {
                    continue; // no drive
                }
                let mut ok = false;
                for _ in 0..100_000 {
                    if inb(base + 7) & 0x80 == 0 {
                        ok = true;
                        break;
                    }
                }
                if !ok {
                    let _ = write!(w, "ch{} dr{} busy; ", ch, dr);
                    continue;
                }
                outb(base + 7, 0xE0); // STANDBY IMMEDIATE
                crate::wdt::delay_ms(50);
                let mut done = false;
                for _ in 0..15 {
                    if inb(base + 7) & 0x80 == 0 {
                        done = true;
                        break;
                    }
                    crate::wdt::pet();
                    crate::wdt::delay_ms(100);
                }
                let _ = write!(w, "ch{} dr{} standby {} (status {:#x}); ", ch, dr, if done { "ok" } else { "pending" }, inb(base + 7));
            }
        }
    }
}

pub fn cmd<'a>(w: &mut dyn Write, it: &mut impl Iterator<Item = &'a str>) {
    match it.next().unwrap_or("") {
        "pci" => {
            pci::walk(&mut |b, id| {
                let _ = write!(w, "{} {:04x}:{:04x} class {:06x}; ", b, id & 0xFFFF, id >> 16, pci::read32(b, 0x08) >> 8);
                false
            });
        }
        "msr" => match it.next().and_then(|x| u32::from_str_radix(x.trim_start_matches("0x"), 16).ok()) {
            Some(m @ (0x17 | 0xCE | 0xE2 | 0x198 | 0x199 | 0x1A0 | 0x19C)) => {
                let _ = write!(w, "msr {:#x} = {:#x}", m, rdmsr(m));
            }
            _ => {
                let _ = write!(w, "err msr: 17 ce e2 198 199 19c 1a0 only");
            }
        },
        "pstate" => match it.next().and_then(|x| u16::from_str_radix(x.trim_start_matches("0x"), 16).ok()) {
            Some(v) if cpuid1_ecx() & (1 << 7) != 0 => {
                let old = rdmsr(0x199);
                let new = old & !0xFFFF | v as u64;
                unsafe { asm!("wrmsr", in("ecx") 0x199u32, in("eax") new as u32, in("edx") (new >> 32) as u32, options(nostack)) };
                crate::wdt::delay_ms(10);
                let _ = write!(w, "perf_ctl {:#x} -> {:#x}, perf_status {:#x}", old, new, rdmsr(0x198));
            }
            Some(_) => {
                let _ = write!(w, "err no EIST");
            }
            None => {
                let _ = write!(w, "err pstate <hex>");
            }
        },
        "cstate" => {
            let h = it.next().unwrap_or("");
            let r = if h == "hlt" { crate::idle::set_mwait(None) } else {
                match u32::from_str_radix(h.trim_start_matches("0x"), 16) {
                    Ok(v) => crate::idle::set_mwait(Some(v)),
                    Err(_) => Err("cstate <hex hint>|hlt"),
                }
            };
            let _ = match r {
                Ok(()) => write!(w, "idle uses {}", if h == "hlt" { "hlt" } else { "mwait" }),
                Err(e) => write!(w, "err {}", e),
            };
        }
        "d3" => match it.next().and_then(parse_bdf) {
            Some(b) => {
                let class = pci::read32(b, 0x08) >> 16;
                if class != 0x0280 && class != 0x0C03 {
                    let _ = write!(w, "err only WiFi (0280) or USB (0c03) controllers, not {:04x}", class);
                    return;
                }
                let mut cap = (pci::read32(b, 0x34) & 0xFC) as u8;
                for _ in 0..16 {
                    if cap == 0 {
                        break;
                    }
                    let h = pci::read32(b, cap);
                    if h & 0xFF == 1 {
                        let cs = pci::read32(b, cap + 4);
                        pci::write32(b, cap + 4, cs & !3 | 3);
                        crate::wdt::delay_ms(10);
                        let _ = write!(w, "{} pmcsr {:#x} -> {:#x}", b, cs & 0xFFFF, pci::read32(b, cap + 4) & 0xFFFF);
                        return;
                    }
                    cap = (h >> 8 & 0xFC) as u8;
                }
                let _ = write!(w, "err {} has no PM capability", b);
            }
            None => {
                let _ = write!(w, "err d3 <bus:dev.fn> (see pwr pci)");
            }
        },
        "panel" => {
            let on = it.next() != Some("off");
            let _ = match crate::gma::panel(on) {
                Some((c, st)) => write!(w, "PP_CONTROL {:#x} PP_STATUS {:#x}", c, st),
                None => write!(w, "err no GMA"),
            };
        }
        "uhci" => {
            // halt every UHCI schedule (USBCMD.RS = 0): no more 1 kHz frame-list DMA keeping the bus busy
            pci::walk(&mut |b, _| {
                if pci::read32(b, 0x08) >> 8 == 0x0C0300 {
                    let io = (pci::read32(b, 0x20) & 0xFFE0) as u16;
                    unsafe {
                        let c = crate::port::inw(io);
                        crate::port::outw(io, c & !1);
                    }
                    crate::wdt::delay_ms(5);
                    let _ = write!(w, "{} io {:#x} status {:#x}; ", b, io, unsafe { crate::port::inw(io + 2) });
                }
                false
            });
        }
        "hdd" => hdd(w, true),
        "disk" => hdd(w, false),
        "bl" => match it.next().and_then(|x| x.parse::<u32>().ok()).filter(|&p| p <= 100) {
            Some(p) => {
                let _ = match crate::gma::backlight(p) {
                    Some((old, new)) => write!(w, "BLC_PWM_CTL {:#x} -> {:#x}", old, new),
                    None => write!(w, "err no GMA / backlight PWM not set up"),
                };
            }
            None => {
                let _ = write!(w, "err bl <0-100>");
            }
        },
        _ => {
            let _ = write!(w, "err pwr pci|msr <hex>|hdd|d3 <bdf>|pstate <hex>|cstate <hex>|hlt|bl <0-100>");
        }
    }
}
