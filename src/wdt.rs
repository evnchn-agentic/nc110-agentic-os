//! TCO watchdog (ICH9 in QEMU q35, NM10 on the NC110; both iTCO v2). boot.s arms it before any
//! Rust runs; this module reports what boot.s did, pets it, and runs the M0.5 rehearsal modes.
use crate::port::{inl, inw, outb, outw};
use crate::{kprintln, pci};
use core::ptr::{addr_of, read_volatile};

#[cfg(all(feature = "neg-control", feature = "grub-armed"))]
compile_error!("neg-control is QEMU-only and must never be combined with grub-armed (the hardware artifact)");

/// Negative control (`--features neg-control`, QEMU q35 only; boot.s never self-arms on NM10): boot.s SETS GCS.NO_REBOOT instead of clearing it
/// (QEMU's GCS powers up 0 with noreboot=false, so merely skipping the clear would prove nothing).
#[no_mangle]
static TCO_NEG_CONTROL: u8 = cfg!(feature = "neg-control") as u8;

/// `--features grub-armed`: boot.s does NOT arm; used to test the GRUB iorw/memrw arming sequence.
#[no_mangle]
static TCO_GRUB_ARMED: u8 = cfg!(feature = "grub-armed") as u8;

extern "C" {
    static tco_result: u32;
    static tco_gcs_pre: u32;
    static tco_gcs: u32;
    static tco_base: u32;
    static tco_sts1_pre: u16;
    static tco_sts2_pre: u16;
    static tco_cnt: u16;
}

fn rd<T: Copy>(p: *const T) -> T {
    unsafe { read_volatile(p) }
}

pub fn armed() -> bool {
    matches!(rd(addr_of!(tco_result)), 0 | 4 | 5)
}

pub fn pmbase() -> u16 {
    (pci::read32(pci::b0(31, 0), 0x40) & 0xFF80) as u16
}

pub fn base() -> u16 {
    pmbase() + 0x60
}

pub fn report() {
    let r = rd(addr_of!(tco_result));
    let what = match r {
        0 => "armed by boot.s",
        1 => "skipped (LPC 00:1f.0 is not ICH9 8086:2918 / NM10 8086:27bc)",
        2 => "NOT armed: PMBASE unset",
        3 => "NOT armed: RCBA disabled",
        4 => "left to GRUB (grub-armed build); live registers follow",
        5 => "NM10: boot.s does not arm (attended Rust arms it; else GRUB must); live registers follow",
        _ => "NOT armed: unknown",
    };
    kprintln!("wdt: {} (result={})", what, r);
    if r == 4 || r == 5 {
        let b = base();
        let gcs = unsafe { read_volatile(((pci::read32(pci::b0(31, 0), 0xF0) & 0xFFFF_C000) + 0x3410) as *const u32) };
        let (s1, s2, cnt, tmr) = unsafe { (inw(b + 4), inw(b + 6), inw(b + 8), inw(b + 0x12)) };
        kprintln!(
            "wdt: live TCOBASE={:#x} GCS={:#010x} NO_REBOOT={} TCO1_CNT={:#06x} HLT={} TCO1_STS={:#06x} TCO2_STS={:#06x} TCO_TMR={}",
            b, gcs, (gcs >> 5) & 1, cnt, (cnt >> 11) & 1, s1, s2, tmr & 0x3FF
        );
        return;
    }
    if r != 0 {
        return;
    }
    let (b, gcs, s1, s2, cnt) = {
        (rd(addr_of!(tco_base)) as u16, rd(addr_of!(tco_gcs)), rd(addr_of!(tco_sts1_pre)), rd(addr_of!(tco_sts2_pre)), rd(addr_of!(tco_cnt)))
    };
    kprintln!(
        "wdt: TCOBASE={:#x} GCS {:#010x} -> {:#010x} NO_REBOOT={} TCO1_CNT={:#06x} HLT={} LOCK={}",
        b, rd(addr_of!(tco_gcs_pre)), gcs, (gcs >> 5) & 1, cnt, (cnt >> 11) & 1, (cnt >> 12) & 1
    );
    kprintln!(
        "wdt: before-clear TCO1_STS={:#06x} TCO2_STS={:#06x} SECOND_TO_STS={} BOOT_STS={}",
        s1, s2, (s2 >> 1) & 1, (s2 >> 2) & 1
    );
    let tmr = unsafe { inw(b + 0x12) } & 0x3FF;
    kprintln!("wdt: TCO_TMR={} ticks x 0.6 s, reset expected after 2nd expiry (~{} ms)", tmr, tmr as u32 * 1200);
}

/// Live check of the reset path, whoever armed it: NO_REBOOT=0, HLT=0, TMR>=4, and the counter
/// (TCO_RLD reads back the current value on v2) decrements across ~1.3 ticks.
pub fn verify_live() -> bool {
    if !armed() {
        return false;
    }
    let b = base();
    let rcba = pci::read32(pci::b0(31, 0), 0xF0);
    if rcba & 1 == 0 {
        return false;
    }
    let gcs = unsafe { read_volatile(((rcba & 0xFFFF_C000) + 0x3410) as *const u32) };
    let (cnt, tmr) = unsafe { (inw(b + 8), inw(b + 0x12) & 0x3FF) };
    let c0 = unsafe { inw(b) } & 0x3FF;
    delay_ms(800);
    let c1 = unsafe { inw(b) } & 0x3FF;
    let ok = gcs & 0x20 == 0 && cnt & 0x0800 == 0 && tmr >= 4 && c1 != c0;
    kprintln!(
        "wdt: verify NO_REBOOT={} HLT={} TMR={} count {}->{} => {}",
        (gcs >> 5) & 1, (cnt >> 11) & 1, tmr, c0, c1, if ok { "VERIFIED" } else { "NOT VERIFIED" }
    );
    ok
}

/// QEMU `-machine pc` (PIIX3 8086:7000 at 00:01.0) has no TCO; the M0 NIC demo may run unprotected there.
pub fn is_qemu_pc() -> bool {
    pci::read32(pci::b0(1, 0), 0) == 0x7000_8086 && rd(addr_of!(tco_result)) == 1
}

/// The negative control must reach its hang unverified (that is the point); only on q35, never NM10.
pub fn neg_control_on_q35() -> bool {
    let on = cfg!(feature = "neg-control") && rd(addr_of!(tco_result)) == 0;
    if on {
        kprintln!("wdt: neg-control build on q35: proceeding unverified on purpose");
    }
    on
}

/// Hard reset via the reset control register (spec M2 `reboot`): back to the firmware / Slot A.
pub fn hard_reset() -> ! {
    kprintln!("wdt: hard reset via 0xCF9");
    unsafe {
        outb(0xCF9, 0x02);
        outb(0xCF9, 0x06);
    }
    loop {
        unsafe { core::arch::asm!("cli; hlt") };
    }
}

/// Attended `wdt=probe`: arm the NM10 TCO ourselves without touching locked bits (SMI_EN.TCO_EN stays as the
/// BIOS left it): clear GCS.NO_REBOOT (the BIOS leaves it SET on the NC110), clear stale status, TMR=50 ticks
/// (30 s per stage, reset ~60 s after the last pet), reload, un-halt. Returns true if the readback says armed.
pub fn arm_probe() -> bool {
    let lpc = pci::read32(pci::b0(31, 0), 0);
    let rcba_raw = pci::read32(pci::b0(31, 0), 0xF0);
    if !(lpc == 0x27BC_8086 || lpc == 0x2918_8086) || pmbase() == 0 || rcba_raw & 1 == 0 {
        kprintln!("wdt: probe arm: prerequisites fail (LPC {:#010x}, PMBASE {:#x}, RCBA {:#x})", lpc, pmbase(), rcba_raw);
        return false;
    }
    let b = base();
    let rcba = rcba_raw & 0xFFFF_C000;
    let gcs_p = (rcba + 0x3410) as *mut u32;
    unsafe {
        let g0 = read_volatile(gcs_p);
        let (cnt0, tmr0, rld0) = (inw(b + 8), inw(b + 0x12) & 0x3FF, inw(b) & 0x3FF);
        kprintln!("wdt: inherited: GCS {:#x} NO_REBOOT={} TCO1_CNT={:#06x} HLT={} TMR={} RLD={} ({})", g0, (g0 >> 5) & 1, cnt0,
            (cnt0 >> 11) & 1, tmr0, rld0, if g0 & 0x20 == 0 && cnt0 & 0x800 == 0 { "inherited enabled: NO_REBOOT clear, timer running" } else { "inherited not enabled" });
        core::ptr::write_volatile(gcs_p, g0 & !0x20);
        outw(b + 4, 0x0008); // TCO1_STS.TIMEOUT (RW1C)
        outw(b + 6, 0x0002); // TCO2_STS.SECOND_TO_STS
        outw(b + 6, 0x0004); // TCO2_STS.BOOT_STS
        outw(b + 0x12, (inw(b + 0x12) & !0x3FF) | 50);
        outw(b, 1);
        outw(b + 8, inw(b + 8) & !0x0800); // un-halt; bit 12 (lock) written back as read
        let (g1, cnt, tmr, c0) = (read_volatile(gcs_p), inw(b + 8), inw(b + 0x12) & 0x3FF, inw(b) & 0x3FF);
        // The count drops every 0.6 s: poll for the first change (boot time) instead of a fixed 1.3 s, same bound.
        let mut c1 = c0;
        for _ in 0..130 {
            delay_ms(10);
            c1 = inw(b) & 0x3FF;
            if c1 != c0 {
                break;
            }
        }
        let ok = g1 & 0x20 == 0 && cnt & 0x0800 == 0 && tmr == 50 && c1 != c0;
        kprintln!("wdt: probe arm: GCS {:#x} -> {:#x} TCO1_CNT={:#06x} TMR={} RLD {}->{} SMI_EN={:#x} => {}",
            g0, g1, cnt, tmr, c0, c1, inl(pmbase() + 0x30), if ok { "ARMED" } else { "NOT ARMED" });
        ok
    }
}

pub fn pet() {
    unsafe { outw(base(), 1) };
}

/// ACPI PM timer (PMBASE+0x08, 3.579545 MHz, 24-bit) based delay; same block on ICH9 and NM10.
fn pm_timer() -> u32 {
    unsafe { inl(pmbase() + 8) & 0x00FF_FFFF }
}

pub fn delay_ms(ms: u32) {
    let need = ms as u64 * 3_579_545 / 1000;
    let mut last = pm_timer();
    let mut acc = 0u64;
    while acc < need {
        let now = pm_timer();
        acc += (now.wrapping_sub(last) & 0x00FF_FFFF) as u64;
        last = now;
    }
}

/// `wdt=pet:N` pets every 500 ms for N s, then stops and hangs. `wdt=hang` hangs at once.
pub fn rehearse(cmdline: &str) -> bool {
    let Some(i) = cmdline.find("wdt=") else { return false };
    let arg = cmdline[i + 4..].split(' ').next().unwrap_or("");
    if arg == "panic" {
        panic!("wdt=panic: test panic"); // the panic handler must restart the board by itself (0xCF9), not wait for the TCO
    }
    if arg == "probe" {
        return false; // attended net, handled in main (k5 hung here before the NIC: result 5 counts as armed)
    }
    if !armed() {
        kprintln!("wdt: mode {} requested but watchdog not armed", arg);
        return false;
    }
    let secs: u32 = arg.strip_prefix("pet:").and_then(|n| n.parse().ok()).unwrap_or(0);
    for s in 0..secs {
        for _ in 0..2 {
            pet();
            delay_ms(500);
        }
        kprintln!("wdt: alive t={}s (petting every 500 ms)", s + 1);
    }
    kprintln!("wdt: HANG now: cli; hlt with the timer running, nobody pets");
    loop {
        unsafe { core::arch::asm!("cli; hlt") };
    }
}
