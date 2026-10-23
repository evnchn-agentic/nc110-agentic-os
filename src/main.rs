#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::fmt::Write;
use core::panic::PanicInfo;

global_asm!(include_str!("boot.s"), options(att_syntax));

mod acpi;
mod console;
mod ec;
mod font;
mod gma;
mod hda;
mod idle;
mod pwr;
mod wifi;
mod iwl;
mod wpa;
mod wg;
mod gfx;
mod qr;
#[allow(dead_code)]
mod dhcp;
mod kbd;
mod mb;
mod net;
mod nic;
mod nic_regs;
mod pci;
mod port;
mod rng;
mod sleep;
mod ssh;
mod bmctl;
mod loader;
mod term;
mod time;
mod usb;
mod udplog;
mod wdt;

struct SerialLog;
impl log::Log for SerialLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            kprintln!("[{}] {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}
static LOGGER: SerialLog = SerialLog;

#[no_mangle]
pub extern "C" fn kmain(magic: u32, info: u64) -> ! {
    console::init();
    // Attended: arm the TCO before anything else can block (Codex night review MUST-FIX 1). No net -> no run.
    if cfg!(feature = "attended") && !wdt::arm_probe() {
        kprintln!("wdt: ATTENDED: could not arm the TCO, refusing to run");
        wdt::hard_reset();
    }
    kprintln!("NC110 bare-metal M3: hello from our own x86_64 kernel (Rust, no_std)");
    if let Some(b) = option_env!("NC110_BUILD") {
        kprintln!("build: {}", b);
    }
    kprintln!("multiboot magic={:#010x} info={:#x} -> long mode OK", magic, info);
    wdt::report();
    if cfg!(feature = "attended") {
        kprintln!("wdt: ATTENDED build: TCO armed by us with NO_REBOOT cleared (any reset -> GRUB default = the agent OS; crash loop -> Slot A)");
    } else if !wdt::is_qemu_pc() && !wdt::verify_live() && !wdt::neg_control_on_q35() {
        kprintln!("wdt: reset path not verified on this machine, refusing to run");
        wdt::hard_reset();
    }
    // Key module right after the watchdog gate: copy the seed out and wipe the module before anything else.
    let client_seed = mb::take_client_key(info);
    if let Some(t) = mb::take_config(info) {
        bmctl::boot_config(t);
    }
    mb::parse_memory(info);
    if let Some(fb) = mb::framebuffer(info) {
        let ok = console::use_framebuffer(fb);
        kprintln!("fb: {}x{} pitch {} {} bpp at {:#x} rgb(pos,size) {:?}: {}", fb.w, fb.h, fb.pitch, fb.bytes * 8, fb.addr, fb.rgb,
            if ok { "native-resolution console" } else { "NOT usable, staying on VGA text" });
    }
    let cmdline = mb::cmdline(info);
    kprintln!("cmdline: {}", cmdline);
    if cmdline.split(' ').any(|w| w == "verbose") {
        console::show_log();
    }
    console::status("starting");
    // A stale FACS waking vector (only a sleeping OS sets one) must never outlive the boot that set it.
    {
        match acpi::clear_waking_vector() {
            Some((v, x)) => kprintln!("acpi: FACS waking vector was {:#x} (X {:#x}){}", v, x, if v != 0 || x != 0 { ", cleared" } else { "" }),
            None => kprintln!("acpi: no FACS"),
        }
    }
    wdt::rehearse(cmdline);

    let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(log::LevelFilter::Warn));
    kprintln!("time: TSC {} ticks/ms (PIT ch2 calibrated); {} ms since CPU reset", time::init(), time::since_reset_ms());
    // GRUB t7 passes the TSC at its entry start and just before `multiboot`: split firmware / GRUB / our early boot.
    if let Some((g0, g1)) = cmdline.split(' ').find_map(|w| w.strip_prefix("grubtsc=")).and_then(|v| {
        let mut it = v.split(',').map(|x| u64::from_str_radix(x.trim_start_matches("0x"), 16).ok());
        Some((it.next()??, it.next()??))
    }) {
        let (a, b, k) = (time::tsc_to_ms(g0), time::tsc_to_ms(g1), time::since_reset_ms());
        kprintln!("boot: firmware + GRUB core {} ms, GRUB entry {} ms, multiboot load + our start {} ms", a, b.saturating_sub(a), k.saturating_sub(b));
    }

    // Mic capture is on by default in the attended agent OS (`nohda` turns it off); `hdadump` also logs every widget.
    if cfg!(feature = "attended") && !cmdline.split(' ').any(|w| w == "nohda") {
        hda::probe(cmdline.split(' ').any(|w| w == "hdadump"));
    }
    if cmdline.split(' ').any(|w| w == "gmadump") {
        gma::dump();
    }
    // Native 1024x600 is the agent OS default (evnchn card 850); `nogma` keeps the VBIOS text mode. native_fb falls back
    // to the VBIOS VGA mode by itself when its checks fail (no LVDS 1024x600 on pipe B, FIFO underrun after the switch).
    let native = cfg!(feature = "attended") && !cmdline.split(' ').any(|w| w == "nogma") || cmdline.split(' ').any(|w| w == "gmafb");
    if !native && cfg!(feature = "attended") {
        gma::restore_vga_if_needed();
    }
    // Emulator only: QEMU's std VGA (Bochs VBE) at the panel's 1024x600, so screen work can be checked pixel-exact in q35.
    // Taken only when that device exists; anywhere else (the NC110) the GMA path below runs exactly as without q35fb.
    let q35 = if cmdline.split(' ').any(|w| w == "q35fb") { q35_fb() } else { None };
    if let Some(fb) = q35 {
        kprintln!("q35fb: Bochs VBE 1024x600x32: {}", console::use_framebuffer(fb));
        if cmdline.split(' ').any(|w| w == "recshow") {
            console::corner(Some(b"REC")); // the overlay as F12 shows it (QEMU has no ALC269 to record from)
        }
    } else if native {
        if let Some(fb) = gma::native_fb() {
            let ok = console::use_framebuffer(fb);
            kprintln!("gma: console on the native framebuffer: {}", ok);
            if !ok {
                gma::undo_native();
            }
        }
    }
    if let Some(f) = cmdline.split(' ').find_map(|w| w.strip_prefix("font=")) {
        if let Some((_, font)) = console::FONTS.iter().find(|(n, _)| *n == f) {
            console::set_font(font);
        }
    }
    {
        console::splash(); // the framebuffer (if any) starts blank: draw the boot screen at its grid
        console::status("starting");
        kprintln!("console: {}x{} cells", console::cols(), console::rows());
    }
    if cmdline.split(' ').any(|w| w == "phyreset") {
        nic::force_phy_reset();
    }
    let mut no_net = false; // no Ethernet link: restart fast, so the day's 6th boot (recovery Linux, WiFi) comes soon
    match nic::probe(cmdline.contains("nic=mmio")) {
        Ok(mut n) => {
            if cmdline.contains("nic=no8105e") {
                n.skip_8105e(); // A/B switch for the rtl_hw_start_8105e tweaks
            }
            kprintln!("nic: {} at {} {} mac={}", n.name(), n.bdf, n.bus, dhcp::Mac(n.mac));
            match n.init() {
                Ok(()) if cfg!(feature = "attended") => {
                    kprintln!("udplog: broadcasting this log to UDP 5514");
                    udplog::set_nic(&n);
                    udplog::dump(1);
                    console::status("network");
                    if cmdline.contains("wdt=probe") {
                        wdt::pet();
                        kprintln!("wdt: HANG now (probe): cli; hlt, nobody pets; expect a reset ~60 s from here");
                        udplog::dump(3);
                        loop {
                            unsafe { asm!("cli; hlt") };
                        }
                    }
                    if let (Some(_), true, true) = (client_seed, cmdline.contains("bmctl"), cmdline.split(' ').any(|w| w == "ssh")) {
                        ssh::run(n, client_seed, cmdline.contains("rng=dump"), Some(info));
                    } else if let (Some(seed), true) = (client_seed, cmdline.contains("bmctl")) {
                        bmctl::run(n, seed, info);
                    } else if client_seed.is_some() {
                        ssh::run(n, client_seed, cmdline.contains("rng=dump"), None);
                    }
                    kprintln!("attended: final log round, then reset");
                    udplog::dump(4);
                }
                Ok(()) => ssh::run(n, client_seed, cmdline.contains("rng=dump"), None),
                Err(e) => {
                    kprintln!("nic: init failed: {} (bus mastering left off)", e);
                    no_net = true;
                }
            }
        }
        Err(e) => {
            kprintln!("nic: {}", e);
            no_net = true;
        }
    }
    kprintln!("DONE");
    if cfg!(feature = "attended") {
        let wait = if no_net { 5 } else { 60 };
        kprintln!("attended: hard reset via 0xCF9 in {} s", wait);
        console::status(if no_net { "no network cable: restarting (after a few tries: Linux with WiFi)" } else { "restarting" });
        for _ in 0..wait {
            wdt::pet();
            wdt::delay_ms(1000);
        }
        wdt::hard_reset();
    }
    loop {
        unsafe { asm!("cli; hlt") };
    }
}

pub fn ram_overlaps(base: u64, len: u64) -> bool {
    mb::ram_overlaps(base, len)
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Restart by ourselves via 0xCF9 instead of waiting for the TCO: on the NC110 a TCO reset has left the board dark
    // since the native-resolution default (2026-10-21, twice), while 0xCF9 resets still came back. A panic while
    // panicking (e.g. in the console) goes straight to the reset.
    static PANICKING: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
    if !PANICKING.swap(true, core::sync::atomic::Ordering::SeqCst) {
        console::show_log();
        let _ = writeln!(console::Console, "PANIC: {}", info);
        let _ = writeln!(console::Console, "restarting in 10 s");
        kprintln!("PANIC: {}", info);
        udplog::dump(9); // the screen alone loses it at the restart (2026-10-22 08:30)
        for _ in 0..20 {
            wdt::pet();
            wdt::delay_ms(500);
        }
        wdt::hard_reset();
    }
    unsafe {
        asm!("out dx, al", in("dx") 0xCF9u16, in("al") 0x02u8);
        asm!("out dx, al", in("dx") 0xCF9u16, in("al") 0x06u8);
    }
    loop {
        unsafe { asm!("cli; hlt") };
    }
}

/// QEMU std VGA (PCI 1234:1111): Bochs VBE dispi registers, 1024x600x32 with the linear framebuffer at BAR0.
fn q35_fb() -> Option<console::Fb> {
    let b = pci::find(0x1234, 0x1111)?;
    let lfb = pci::read32(b, 0x10) as u64 & !0xF;
    let w = |i: u16, v: u16| unsafe {
        port::outw(0x1CE, i);
        port::outw(0x1CF, v);
    };
    w(4, 0); // disable while changing the mode
    w(1, 1024);
    w(2, 600);
    w(3, 32);
    w(6, 1024); // virtual width
    w(4, 0x41); // enabled | linear framebuffer
    Some(console::Fb { addr: lfb, pitch: 4096, w: 1024, h: 600, bytes: 4, rgb: [(16, 8), (8, 8), (0, 8)] })
}
