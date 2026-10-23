//! Intel GMA 3150 (Pineview, 00:02.0) display, step 1: read-only. Logs the pipe/plane/panel-fitter/LVDS registers the
//! VBIOS left (cmdline `gmadump`), to plan a native 1024x600 plane under the VGA text mode the panel fitter scales
//! today. Nothing is written except marking the register pages uncached.
use crate::{kprintln, nic, pci};
use core::ptr::read_volatile;

const REGS: [(u32, &str); 22] = [
    (0x61180, "LVDS"), (0x61200, "PP_STATUS"), (0x61230, "PFIT_CONTROL"), (0x61234, "PFIT_PGM_RATIOS"),
    (0x60000, "HTOTAL_A"), (0x60008, "HSYNC_A"), (0x6000c, "VTOTAL_A"), (0x60014, "VSYNC_A"), (0x6001c, "PIPEASRC"),
    (0x61000, "HTOTAL_B"), (0x61008, "HSYNC_B"), (0x6100c, "VTOTAL_B"), (0x61014, "VSYNC_B"), (0x6101c, "PIPEBSRC"),
    (0x70008, "PIPEACONF"), (0x71008, "PIPEBCONF"), (0x70180, "DSPACNTR"), (0x71180, "DSPBCNTR"),
    (0x71188, "DSPBSTRIDE"), (0x7119c, "DSPBSURF"), (0x71190, "DSPBSIZE"), (0x71400, "VGACNTRL"),
];

pub fn dump() {
    let Some(b) = pci::find(0x8086, 0xA011).or_else(|| pci::find(0x8086, 0xA001)) else {
        kprintln!("gma: no Pineview GMA at 00:02.0");
        return;
    };
    let bar = pci::read32(b, 0x10) as u64 & !0xF; // MMADR: registers (512 KiB)
    let gmadr = pci::read32(b, 0x18) as u64 & !0xF; // aperture (graphics memory)
    kprintln!("gma: {:#010x} MMADR {:#x} GMADR {:#x} BSM {:#x} GMCH_CTRL {:#x}", pci::read32(b, 0), bar, gmadr,
        pci::read32(b, 0x5C), pci::read32(pci::b0(0, 0), 0x50));
    for (off, name) in REGS {
        if !nic::map_uc_4k(bar + off as u64) {
            kprintln!("gma: no UC table for {:#x}", off);
            return;
        }
        let v = unsafe { read_volatile((bar + off as u64) as *const u32) };
        kprintln!("gma: {:<17} {:#07x} = {:#010x}", name, off, v);
    }
}

fn rd(bar: u64, off: u32) -> u32 {
    unsafe { read_volatile((bar + off as u64) as *const u32) }
}
fn wr(bar: u64, off: u32, v: u32) {
    unsafe { core::ptr::write_volatile((bar + off as u64) as *mut u32, v) }
}

/// Native 1024x600 (cmdline `gmafb`): keep the VBIOS's LVDS timing on pipe B (already the panel's 1024x600), but feed
/// it from display plane B at 32 bpp out of the graphics aperture instead of the VGA plane the panel fitter stretches
/// (640x400 -> 1024x600). Pineview is gen3: the plane base is DSPBADDR (a GTT offset; 0 = aperture start, the memory
/// GRUB's VESA modes also used). Only proceeds if pipe B drives the LVDS at 1024x600; a reboot restores the VBIOS mode.
pub fn native_fb() -> Option<crate::console::Fb> {
    let b = pci::find(0x8086, 0xA011).or_else(|| pci::find(0x8086, 0xA001))?;
    let bar = pci::read32(b, 0x10) as u64 & !0xF;
    let gmadr = pci::read32(b, 0x18) as u64 & !0xF;
    for off in [0x61000u32, 0x61180, 0x61200, 0x70000, 0x71000, 0x71400] { // 0x61000 page also holds 0x61230
        if !nic::map_uc_4k(bar + off as u64) {
            return None;
        }
    }
    let (htot, vtot, lvds) = (rd(bar, 0x61000), rd(bar, 0x6100c), rd(bar, 0x61180));
    let (w, h) = ((htot & 0xFFF) + 1, (vtot & 0xFFF) + 1);
    if lvds & (1 << 31) == 0 || lvds & (1 << 30) == 0 || (w, h) != (1024, 600) {
        kprintln!("gma: native fb refused: LVDS {:#x}, pipe B active {}x{}", lvds, w, h);
        return None;
    }
    pci::write32(b, 4, pci::read32(b, 4) & 0xFFFF | 0x6); // memory + bus master (display fetch)
    // Uncached BEFORE the clear (review MF-D): clearing through the boot's write-back mapping could leave dirty lines that
    // a later eviction writes over pixels drawn uncached.
    if !nic::map_fb(gmadr, 4096 * 600) {
        kprintln!("gma: aperture {:#x} could not be mapped uncached: staying on VGA text", gmadr);
        return None;
    }
    // Clear the first screen of the aperture before the plane shows it.
    for i in 0..(1024 * 600) as u64 {
        unsafe { core::ptr::write_volatile((gmadr + i * 4) as *mut u32, 0) };
    }
    // Linux's i9xx order: planes off, pipe off (wait for it), then panel fitter and source size, pipe on, plane on.
    // Attempt 1 changed PIPEBSRC with the pipe running: the fitter ignored the disable and kept its 640x400 ratios,
    // and PIPEBSTAT showed a FIFO underrun (black screen).
    wr(bar, 0x71400, rd(bar, 0x71400) | (1 << 31)); // VGACNTRL: VGA plane off
    wr(bar, 0x71180, rd(bar, 0x71180) & !(1 << 31)); // plane B off
    wr(bar, 0x71184, 0);
    // gen3 PIPECONF has no pipe-state bit (it read 0x80000000 throughout attempt 2): wait two frames instead.
    let pf0 = (rd(bar, 0x61230), rd(bar, 0x61234), rd(bar, 0x61238));
    wr(bar, 0x71008, rd(bar, 0x71008) & !(1 << 31)); // PIPEBCONF: pipe off
    crate::wdt::delay_ms(40);
    wr(bar, 0x61230, 0); // PFIT_CONTROL (0x61230 on gen3/4; 0x61200 is PP_STATUS): fitter off, source = native
    wr(bar, 0x6101c, (1023 << 16) | 599); // PIPEBSRC
    wr(bar, 0x71008, rd(bar, 0x71008) | (1 << 31)); // pipe on
    crate::wdt::delay_ms(40);
    kprintln!("gma: PFIT ctl/pgm/auto {:x?} -> ctl {:#x}", pf0, rd(bar, 0x61230));
    wr(bar, 0x71188, 4096); // DSPBSTRIDE
    wr(bar, 0x71190, (599 << 16) | 1023); // DSPBSIZE
    wr(bar, 0x7118c, 0); // DSPBPOS
    // DSPBCNTR: on, 32 bpp xRGB, pipe B. Gamma (bit 30) stays OFF: the VBIOS's palette is the 8-bit VGA one, and through
    // it the 32 bpp colours came out wrong (cyan white, grey chips white; camera shot proof/cam-native-gamma-on.jpg).
    wr(bar, 0x71180, (1 << 31) | (6 << 26) | (1 << 24));
    wr(bar, 0x71184, 0); // DSPBADDR: aperture offset 0 (writing it latches the plane registers)
    wr(bar, 0x71024, rd(bar, 0x71024) | (1 << 31)); // PIPEBSTAT: clear the underrun flag (RW1C) so `gma` shows new ones
    crate::wdt::delay_ms(50);
    if rd(bar, 0x71024) & (1 << 31) != 0 {
        kprintln!("gma: FIFO underrun after the switch: back to the VBIOS VGA mode");
        restore_vga_regs(bar);
        return None;
    }
    kprintln!("gma: native 1024x600 plane B on: DSPBCNTR {:#x} PIPEBSRC {:#x} PFIT {:#x} VGACNTRL {:#x}", rd(bar, 0x71180),
        rd(bar, 0x6101c), rd(bar, 0x61230), rd(bar, 0x71400));
    Some(crate::console::Fb { addr: gmadr, pitch: 4096, w: 1024, h: 600, bytes: 4, rgb: [(16, 8), (8, 8), (0, 8)] })
}

/// Live display state for bmctl `gma` (the screen cannot be seen from here, so these are the checks):
/// pipe B conf/status (bit 31 of PIPEBSTAT = FIFO underrun), panel fitter, plane B control/base/stride, the FIFO
/// watermarks, GTT entry 0, and whether a pixel written through the aperture reads back from stolen memory.
pub fn state(w: &mut dyn core::fmt::Write) {
    let Some(b) = pci::find(0x8086, 0xA011).or_else(|| pci::find(0x8086, 0xA001)) else {
        let _ = write!(w, "no gma");
        return;
    };
    let bar = pci::read32(b, 0x10) as u64 & !0xF;
    let gmadr = pci::read32(b, 0x18) as u64 & !0xF;
    let gtt = pci::read32(b, 0x1C) as u64 & !0xF;
    let bsm = pci::read32(b, 0x5C) as u64 & !0xFFFFF;
    for off in [0x61000u32, 0x61200, 0x70000, 0x71000, 0x71400] {
        let _ = nic::map_uc_4k(bar + off as u64);
    }
    let _ = nic::map_uc_4k(gtt);
    let _ = nic::map_uc_4k(bsm); // read stolen memory uncached, or the round trip compares against a stale line
    let pstat = rd(bar, 0x71024);
    let g0 = unsafe { read_volatile(gtt as *const u32) };
    // pixel round trip: aperture offset 0 vs stolen memory start (what GTT entry 0 should point at)
    let probe = 0x5A5A_0000 | (crate::time::now_ms() as u32 & 0xFFFF);
    let old = unsafe { read_volatile(gmadr as *const u32) };
    unsafe { core::ptr::write_volatile(gmadr as *mut u32, probe) };
    let back = unsafe { read_volatile(bsm as *const u32) };
    unsafe { core::ptr::write_volatile(gmadr as *mut u32, old) };
    let _ = write!(w, "PIPEBCONF {:#x} PIPEBSTAT {:#x} (underrun {}) PIPEBSRC {:#x} PFIT {:#x}/{:#x} PP_STATUS {:#x} DSPBCNTR {:#x} DSPBADDR {:#x} \
        DSPBSTRIDE {} DSPBSIZE {:#x} VGACNTRL {:#x} DSPARB {:#x} DSPFW1 {:#x} DSPFW2 {:#x} DSPFW3 {:#x} GTT0 {:#x} BSM {:#x} \
        aperture->stolen {}", rd(bar, 0x71008), pstat, pstat >> 31, rd(bar, 0x6101c), rd(bar, 0x61230), rd(bar, 0x61238), rd(bar, 0x61200),
        rd(bar, 0x71180), rd(bar, 0x71184), rd(bar, 0x71188), rd(bar, 0x71190), rd(bar, 0x71400), rd(bar, 0x70030), rd(bar, 0x70034),
        rd(bar, 0x70038), rd(bar, 0x7003c), g0, bsm, if back == probe { "OK" } else { "MISMATCH" });
}

/// The VBIOS's setup on the NC110 (proof/nc110-gma-dump.log): VGA plane on pipe B, 640x400 source, panel fitter on.
const VBIOS_PFIT: u32 = 0x8000_2668;
const VBIOS_PIPEBSRC: u32 = 0x027F_018F;

fn restore_vga_regs(bar: u64) {
    wr(bar, 0x71180, rd(bar, 0x71180) & !(1 << 31)); // plane B off
    wr(bar, 0x71184, 0);
    wr(bar, 0x71008, rd(bar, 0x71008) & !(1 << 31)); // pipe off
    crate::wdt::delay_ms(40);
    wr(bar, 0x61230, VBIOS_PFIT);
    wr(bar, 0x6101c, VBIOS_PIPEBSRC);
    wr(bar, 0x71008, rd(bar, 0x71008) | (1 << 31)); // pipe on
    crate::wdt::delay_ms(40);
    wr(bar, 0x71400, rd(bar, 0x71400) & !(1 << 31)); // VGA plane on
    wr(bar, 0x71024, rd(bar, 0x71024) | (1 << 31));
}

/// The console refused the native framebuffer after the switch (review MF-E): put the VBIOS VGA mode back so the text
/// screen is visible.
pub fn undo_native() {
    let Some(b) = pci::find(0x8086, 0xA011).or_else(|| pci::find(0x8086, 0xA001)) else { return };
    restore_vga_regs(pci::read32(b, 0x10) as u64 & !0xF);
}

/// Text-mode boot after a native-resolution kernel (chain-load keeps the display state): if plane B is on and the
/// VGA plane off, put the VBIOS's VGA setup back so the text screen is what the panel shows (not a stale frame).
pub fn restore_vga_if_needed() {
    let Some(b) = pci::find(0x8086, 0xA011).or_else(|| pci::find(0x8086, 0xA001)) else { return };
    let bar = pci::read32(b, 0x10) as u64 & !0xF;
    for off in [0x61000u32, 0x70000, 0x71000, 0x71400] {
        if !nic::map_uc_4k(bar + off as u64) {
            return;
        }
    }
    if rd(bar, 0x71180) & (1 << 31) != 0 && rd(bar, 0x71400) & (1 << 31) != 0 {
        restore_vga_regs(bar);
        kprintln!("gma: inherited native plane: VBIOS VGA mode restored (VGACNTRL {:#x})", rd(bar, 0x71400));
    }
}

/// Backlight duty to `pct` of the PWM period (Pineview BLC_PWM_CTL 0x61254: period in 31:17, duty in 15:1, as i915
/// reads it). Returns (old, new) register values; None without the GMA or with no period programmed.
pub fn backlight(pct: u32) -> Option<(u32, u32)> {
    let b = pci::find(0x8086, 0xA011).or_else(|| pci::find(0x8086, 0xA001))?;
    let bar = pci::read32(b, 0x10) as u64 & !0xF;
    if !nic::map_uc_4k(bar + 0x61000) {
        return None;
    }
    let old = rd(bar, 0x61254);
    let max = old >> 17;
    if max == 0 {
        return None;
    }
    let new = old & 0xFFFF_0001 | (max * pct / 100) << 1;
    wr(bar, 0x61254, new);
    Some((old, rd(bar, 0x61254)))
}

/// LVDS panel power (PP_CONTROL 0x61204 bit 0 = power target; PP_STATUS 0x61200 bit 31 = on): off blanks panel and
/// backlight, on restores them. Returns (control, status) after the change.
pub fn panel(on: bool) -> Option<(u32, u32)> {
    let b = pci::find(0x8086, 0xA011).or_else(|| pci::find(0x8086, 0xA001))?;
    let bar = pci::read32(b, 0x10) as u64 & !0xF;
    if !nic::map_uc_4k(bar + 0x61000) {
        return None;
    }
    let c = rd(bar, 0x61204);
    wr(bar, 0x61204, if on { c | 1 } else { c & !1 });
    crate::wdt::delay_ms(300);
    Some((rd(bar, 0x61204), rd(bar, 0x61200)))
}

/// Pipe B frame counter (PIPEBFRAMEHIGH 0x71040: frame bits 23:8; PIPEBFRAMEPIXEL 0x71044 bits 31:24: frame bits
/// 7:0). It ticks at the start of every vertical blank: pacing on it = the panel's refresh. None = no Pineview GMA.
pub fn frame() -> Option<u32> {
    static mut BAR: u64 = 0;
    unsafe {
        if BAR == 0 {
            let b = pci::find(0x8086, 0xA011).or_else(|| pci::find(0x8086, 0xA001))?;
            let bar = pci::read32(b, 0x10) as u64 & !0xF;
            if !nic::map_uc_4k(bar + 0x71000) {
                return None;
            }
            BAR = bar;
        }
        for _ in 0..4 {
            // high, low, high: a carry between the two reads would pair an old high part with a new low byte
            let h = rd(BAR, 0x71040) & 0xFFFF;
            let l = rd(BAR, 0x71044) >> 24;
            if rd(BAR, 0x71040) & 0xFFFF == h {
                return Some(h << 8 | l);
            }
        }
        None
    }
}

/// The graphics aperture (GMADR, 256 MiB on Pineview) if `base..base+len` rounded out to 2 MiB lies inside it: the
/// only range fb_wc may make write-combining.
pub fn aperture_holds(base: u64, len: u64) -> bool {
    let Some(b) = pci::find(0x8086, 0xA011).or_else(|| pci::find(0x8086, 0xA001)) else { return false };
    let gm = pci::read32(b, 0x18) as u64 & !0xF;
    let (lo, hi) = (base & !0x1F_FFFF, (base + len + 0x1F_FFFF) & !0x1F_FFFF);
    gm != 0 && lo >= gm && hi <= gm + 0x1000_0000
}
