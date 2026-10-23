//! Output to COM1 (0x3F8, 115200 8N1) and an 80x25 cell screen: the VGA text buffer (0xB8000), or, when GRUB set a
//! linear framebuffer (multiboot video mode, the panel's native 1024x600), the same cells drawn at native resolution
//! with the 12x24 font (960x600, centred). Every cell write goes through `cell`, which keeps a shadow copy and only
//! redraws cells that changed.
//! The screen shows a quiet splash (title, one status line, a footer bar) by default; the log still goes to COM1,
//! the RAM capture (bmctl `log`, udplog) and comes back on screen with `show_log` (cmdline `verbose`, panics).
use crate::port::{inb, outb};
use core::fmt;

const COM1: u16 = 0x3F8;
const VGA: *mut u16 = 0xB8000 as *mut u16;
const MAXW: usize = crate::term::MAXW;
const MAXH: usize = crate::term::MAXH;
static mut COLS: usize = 80; // VGA text: 80x25; framebuffer: the panel divided by the font cell
static mut ROWS: usize = 25;
static mut FONT: &crate::font::Font = &crate::font::F12X24;

pub fn is_fb() -> bool {
    unsafe { (*core::ptr::addr_of!(FB)).is_some() }
}

pub fn cols() -> usize {
    unsafe { COLS }
}
pub fn rows() -> usize {
    unsafe { ROWS }
}
fn font() -> &'static crate::font::Font {
    unsafe { FONT }
}
const ATTR: u16 = 0x0700; // the log: light grey on the background (was white on blue)

/// Linear framebuffer from multiboot: address, pitch, width, height, bytes per pixel, (r, g, b) (position, size).
#[derive(Clone, Copy)]
pub struct Fb {
    pub addr: u64,
    pub pitch: u32,
    pub w: u32,
    pub h: u32,
    pub bytes: u32,
    pub rgb: [(u8, u8); 3],
}

static mut FB_WC: bool = false; // the framebuffer is mapped write-combining (use_framebuffer)
static mut FB: Option<(Fb, u32, u32)> = None; // framebuffer, x0, y0 of the 960x600 cell area
static mut SHADOW: [u64; MAXW * MAXH] = [0; MAXW * MAXH]; // term::Cell: char, attribute, exact fg/bg RGB
static mut CURSOR: Option<usize> = None;
static mut POINTER: Option<usize> = None;

/// The mouse pointer cell is shown with its attribute XORed with 0x77 (fg/bg swapped-ish, bright bits kept) and its
/// exact colours swapped.
fn shown(i: usize, v: u64) -> u64 {
    if let Some(c) = corner_cell(i) {
        c as u64
    } else if unsafe { POINTER } == Some(i) {
        (v & 0xFFFF) ^ 0x7700 | (v >> 40 & 0xFF_FFFF) << 16 | (v >> 16 & 0xFF_FFFF) << 40
    } else {
        v
    }
}

/// Move the mouse pointer to cell `pos` (None hides it). The shadow keeps the real cells.
pub fn pointer(pos: Option<usize>) {
    unsafe {
        let old = POINTER;
        if old == pos {
            return;
        }
        POINTER = pos;
        for i in [old, pos].into_iter().flatten() {
            let v = shown(i, cell_get64(i));
            if (*core::ptr::addr_of!(FB)).is_some() {
                draw(i, v, CURSOR == Some(i));
            } else {
                VGA.add(i).write_volatile(v as u16);
            }
        }
    }
}

// Attribute colours 0..15 as RGB on the framebuffer: a dark theme with tested contrast (GitHub dark's palette), used by
// the home screen and the SSH terminal alike. 0 is the background (#0D1117, not pure black); 7 (the terminal's default
// text) is 12:1 on it, 8 (secondary text) 6:1. VGA text mode keeps the hardware palette.
const PAL: [u32; 16] = [0x0D1117, 0x58A6FF, 0x3FB950, 0x39C5CF, 0xF85149, 0xBC8CFF, 0xD29922, 0xC9D1D9, 0x8B949E, 0x79C0FF,
    0x56D364, 0x56D4DD, 0xFF7B72, 0xD2A8FF, 0xE3B341, 0xF0F6FC];

fn pixel(fb: &Fb, rgb: u32) -> u32 {
    let ch = |v: u32, (pos, size): (u8, u8)| (v >> (8 - size.min(8))) << (pos & 31);
    ch(rgb >> 16 & 0xFF, fb.rgb[0]) | ch(rgb >> 8 & 0xFF, fb.rgb[1]) | ch(rgb & 0xFF, fb.rgb[2])
}

fn draw(i: usize, v: u64, underline: bool) {
    let Some((fb, x0, y0)) = (unsafe { *core::ptr::addr_of!(FB) }) else { return };
    let attr = (v >> 8 & 0xFF) as usize;
    let (fr, br) = ((v >> 16 & 0xFF_FFFF) as u32, (v >> 40 & 0xFF_FFFF) as u32); // exact colours (0 = palette)
    let (fg, bg) = (pixel(&fb, if fr != 0 { fr } else { PAL[attr & 0xF] }), pixel(&fb, if br != 0 { br } else { PAL[attr >> 4 & 0xF] }));
    let f = font();
    let g = f.glyph((v & 0xFF) as u8);
    let (cx, cy) = (x0 + (i % cols()) as u32 * f.w as u32, y0 + (i / cols()) as u32 * f.h as u32);
    let big = title_rect().filter(|&(r0, c0, w, h)| unsafe { TITLE_ON }
        && (r0..r0 + h).contains(&(i / cols())) && (c0..c0 + w).contains(&(i % cols())) && cell_get(i) >> 8 == TITLE >> 8);
    // The side margins (1024 - 960 = 2 x 32 px on the NC110) take the background of the edge cell next to them, so
    // the footer bar and coloured terminal rows reach the panel edges.
    let margin = match i % cols() {
        0 => Some((0, x0)),
        c if c == cols() - 1 => Some((cx + f.w as u32, fb.w)),
        _ => None,
    };
    if let Some((mx0, mx1)) = margin {
        for y in 0..f.h as u32 {
            let line = fb.addr + (cy + y) as u64 * fb.pitch as u64;
            for x in mx0..mx1 {
                let a = line + (x * fb.bytes) as u64;
                unsafe {
                    match fb.bytes {
                        4 => core::ptr::write_volatile(a as *mut u32, bg),
                        3 => {
                            core::ptr::write_volatile(a as *mut u16, bg as u16);
                            core::ptr::write_volatile((a + 2) as *mut u8, (bg >> 16) as u8);
                        }
                        _ => core::ptr::write_volatile(a as *mut u16, bg as u16),
                    }
                }
            }
        }
    }
    for (y, &bits) in g.iter().enumerate() {
        let bits = if underline && y + 3 >= f.h && y + 1 < f.h { (1 << f.w) - 1 } else { bits };
        // The title: this cell's slice of TITLE_TEXT drawn TITLE_SCALE times the font size.
        let bits = match big {
            Some((r0, c0, _, _)) => {
                let py = ((i / cols() - r0) * f.h + y) / TITLE_SCALE;
                (0..f.w).fold(0u16, |acc, x| {
                    let px = ((i % cols() - c0) * f.w + x) / TITLE_SCALE;
                    let on = f.glyph(TITLE_TEXT[px / f.w])[py] >> (f.w - 1 - px % f.w) & 1;
                    acc | on << (f.w - 1 - x)
                })
            }
            None => bits,
        };
        let line = fb.addr + (cy + y as u32) as u64 * fb.pitch as u64 + cx as u64 * fb.bytes as u64;
        for x in 0..f.w {
            let p = if bits & (1 << (f.w - 1 - x)) != 0 { fg } else { bg };
            let a = line + (x as u32 * fb.bytes) as u64;
            unsafe {
                match fb.bytes {
                    4 => core::ptr::write_volatile(a as *mut u32, p),
                    3 => {
                        core::ptr::write_volatile(a as *mut u16, p as u16);
                        core::ptr::write_volatile((a + 2) as *mut u8, (p >> 16) as u8);
                    }
                    _ => core::ptr::write_volatile(a as *mut u16, p as u16),
                }
            }
        }
    }
}

/// Set one cell (VGA attribute << 8 | CP437 char). Only a changed cell is redrawn.
pub fn cell(i: usize, v: u16) {
    cell64(i, v as u64)
}

/// A terminal cell with exact colours (term::Cell). Only a changed cell is redrawn.
pub fn cell64(i: usize, v: u64) {
    unsafe {
        let sh = &mut *core::ptr::addr_of_mut!(SHADOW);
        if sh[i] == v {
            return;
        }
        sh[i] = v;
        if let Some((r0, c0, w, h)) = title_rect().filter(|&(r0, c0, w, h)| TITLE_ON && (v as u16) >> 8 != TITLE >> 8
            && (r0..r0 + h).contains(&(i / cols())) && (c0..c0 + w).contains(&(i % cols()))) {
            TITLE_ON = false; // something else (the terminal) owns this area now: repaint the title's cells as cells
            for r in r0..r0 + h {
                for c in c0..c0 + w {
                    draw_now(r * cols() + c);
                }
            }
        }
        let v = shown(i, v);
        if (*core::ptr::addr_of!(FB)).is_some() {
            draw(i, v, CURSOR == Some(i));
        } else {
            VGA.add(i).write_volatile(v as u16);
        }
    }
}

/// Redraw cell `i` from the shadow (framebuffer only).
fn draw_now(i: usize) {
    if is_fb() {
        draw(i, shown(i, cell_get64(i)), unsafe { CURSOR } == Some(i));
    }
}

pub fn cell_get(i: usize) -> u16 {
    unsafe { (*core::ptr::addr_of!(SHADOW))[i] as u16 }
}

pub fn cell_get64(i: usize) -> u64 {
    unsafe { (*core::ptr::addr_of!(SHADOW))[i] }
}

/// Text cursor at cell `pos` (None hides it): the VGA hardware cursor, or an underline on the framebuffer.
pub fn cursor(pos: Option<usize>) {
    unsafe {
        if (*core::ptr::addr_of!(FB)).is_some() {
            let old = CURSOR;
            CURSOR = pos;
            if old != pos {
                if let Some(o) = old {
                    draw(o, shown(o, cell_get64(o)), false);
                }
                if let Some(p) = pos {
                    draw(p, shown(p, cell_get64(p)), true);
                }
            }
            return;
        }
        match pos {
            None => {
                outb(0x3D4, 0x0A);
                outb(0x3D5, 0x20); // bit 5 hides the cursor
            }
            Some(p) => {
                outb(0x3D4, 0x0A);
                outb(0x3D5, 13); // scanlines 13..14
                outb(0x3D4, 0x0B);
                outb(0x3D5, 14);
                outb(0x3D4, 0x0F);
                outb(0x3D5, p as u8);
                outb(0x3D4, 0x0E);
                outb(0x3D5, (p >> 8) as u8);
            }
        }
    }
}

/// Switch the screen to a multiboot linear framebuffer (needs >= 960x600 and 16/24/32 bpp): clear it and redraw every
/// cell from the shadow. False = not usable (the caller keeps VGA text, which a graphics mode will not show).
pub fn use_framebuffer(fb: Fb) -> bool {
    let f = font();
    if fb.w < 80 * f.w as u32 || fb.h < 25 * f.h as u32 || !(2..=4).contains(&fb.bytes) || fb.pitch < fb.w * fb.bytes {
        return false;
    }
    if !crate::nic::map_fb(fb.addr, fb.pitch as u64 * fb.h as u64) {
        return false;
    }
    // write-combining whenever the framebuffer is inside the GMA aperture: uncached, a full herdr redraw took tens of
    // ms and touchpad packets piled up behind it in the SSH loop
    if crate::gma::aperture_holds(fb.addr, fb.pitch as u64 * fb.h as u64) {
        crate::nic::fb_wc(fb.addr, fb.pitch as u64 * fb.h as u64, true);
        unsafe { FB_WC = true };
    }
    unsafe {
        FB = Some((fb, 0, 0));
    }
    set_font(f);
    true
}

/// Framebuffer only: switch the cell font (the grid becomes the panel divided by the cell, up to 170x50) and repaint
/// from the shadow. The callers redraw their screen (splash / terminal) at the new size.
pub fn set_font(f: &'static crate::font::Font) {
    unsafe {
        let Some((fb, _, _)) = *core::ptr::addr_of!(FB) else { return };
        let (c, r) = ((fb.w as usize / f.w).min(MAXW), (fb.h as usize / f.h).min(MAXH));
        FONT = f;
        (COLS, ROWS) = (c, r);
        FB = Some((fb, (fb.w - (c * f.w) as u32) / 2, (fb.h - (r * f.h) as u32) / 2));
        CURSOR = None;
        POINTER = None;
        let bg = pixel(&fb, PAL[0]);
        for y in 0..fb.h as u64 {
            for x in 0..fb.w as u64 {
                let a = fb.addr + y * fb.pitch as u64 + x * fb.bytes as u64;
                match fb.bytes {
                    4 => core::ptr::write_volatile(a as *mut u32, bg),
                    3 => {
                        core::ptr::write_volatile(a as *mut u16, bg as u16);
                        core::ptr::write_volatile((a + 2) as *mut u8, (bg >> 16) as u8);
                    }
                    _ => core::ptr::write_volatile(a as *mut u16, bg as u16),
                }
            }
        }
        (*core::ptr::addr_of_mut!(SHADOW)).fill(0);
    }
}

/// The framebuffer fonts, smallest cell last: (name, font).
pub const FONTS: [(&str, &crate::font::Font); 3] =
    [("12x24", &crate::font::F12X24), ("8x16", &crate::font::F8X16), ("6x12", &crate::font::F6X12)];

/// Cycle to the next font (framebuffer only). Returns the new grid, or None on VGA text.
pub fn next_font() -> Option<(usize, usize)> {
    unsafe {
        (*core::ptr::addr_of!(FB))?;
        let cur = FONTS.iter().position(|(_, f)| core::ptr::eq(*f, FONT)).unwrap_or(0);
        set_font(FONTS[(cur + 1) % FONTS.len()].1);
        Some((COLS, ROWS))
    }
}

static mut ROW: usize = 0;
static mut COL: usize = 0;
/// Log lines also go to the screen (verbose / panic). Off for the splash and the SSH terminal.
static mut VGA_LOG: bool = false;

pub fn vga_log_off() {
    unsafe { VGA_LOG = false };
}

/// Blank the screen in the log colours, replay the last lines of the capture, then keep logging to it.
pub fn show_log() {
    unsafe {
        fill(ATTR);
        ROW = 0;
        COL = 0;
        let c = captured();
        let mut st = c.len();
        for _ in 0..rows() - 1 {
            match c[..st.saturating_sub(1)].iter().rposition(|&b| b == b'\n') {
                Some(i) => st = i + 1,
                None => {
                    st = 0;
                    break;
                }
            }
        }
        for &b in &c[st..] {
            vga_putc(b);
        }
        VGA_LOG = true;
    }
}

fn fill(attr: u16) {
    unsafe { TITLE_ON = false };
    for i in 0..cols() * rows() {
        cell(i, attr | b' ' as u16);
    }
}

fn put(row: usize, col: usize, attr: u16, s: &[u8]) {
    for (i, &b) in s.iter().enumerate().take(cols().saturating_sub(col)) {
        cell(row * cols() + col + i, attr | b as u16);
    }
}

fn put_center(row: usize, attr: u16, s: &[u8], bg: u16) {
    put(row, 0, bg, &[b' '; MAXW][..cols()]);
    put(row, (cols() - s.len().min(cols())) / 2, attr, s);
}

const BG: u16 = 0x0000; // background
const TITLE: u16 = 0x0F00; // near-white (the title area's marker attribute too)
/// Secondary text: palette 8 on the framebuffer (6.2:1); light grey in VGA text mode, whose dark grey is only 2.8:1.
fn dim() -> u16 {
    if is_fb() { 0x0800 } else { 0x0700 }
}
const TEXT: u16 = 0x0F00; // primary text
const KEY: u16 = 0x0900; // accent: keys
const OK: u16 = 0x0A00; // green dot: ready
const REC_DOT: u16 = 0x0C00; // red dot: recording
const BAR: u16 = 0x7000; // dark on light grey: the selected chooser row
const LINE: u16 = 0x0800; // the footer's separator line
/// Splash rows: the 25-row layout, centred vertically on taller grids.
fn splash_row() -> usize {
    rows().saturating_sub(25) / 2 + 6
}
fn status_row() -> usize {
    splash_row() + 7
}

/// The title, drawn TITLE_SCALE x the cell font over a block of cells (framebuffer); plain text in VGA text mode.
const TITLE_TEXT: &[u8] = b"NC110";
const TITLE_SCALE: usize = 3;
static mut TITLE_ON: bool = false;
/// (row, col, cells wide, cells high) of the title.
fn title_rect() -> Option<(usize, usize, usize, usize)> {
    let (w, h) = if is_fb() { (TITLE_TEXT.len() * TITLE_SCALE, TITLE_SCALE) } else { (TITLE_TEXT.len(), 1) };
    Some((splash_row() + if is_fb() { 0 } else { 2 }, cols().checked_sub(w)? / 2, w, h))
}

/// The quiet boot screen: logo, subtitle, empty status line and footer. Log lines stop reaching the screen.
pub fn splash() {
    unsafe {
        VGA_LOG = false;
        CHOOSER = None;
    }
    cursor(None);
    fill(BG);
    if let Some((r0, c0, w, h)) = title_rect() {
        for r in r0..r0 + h {
            put(r, c0, TITLE, &[b' '; MAXW][..w]);
        }
        put(r0, c0, TITLE, TITLE_TEXT); // the text itself in the first row: VGA shows it, bmctl `screen` reads it
        unsafe { TITLE_ON = true };
        for r in r0..r0 + h {
            for c in c0..c0 + w {
                draw_now(r * cols() + c);
            }
        }
    }
    let (mut sub, mut n) = ([0u8; 48], 10);
    sub[..n].copy_from_slice(b"agentic OS");
    if let Some(v) = option_env!("NC110_BUILD") {
        let v = &v.as_bytes()[..v.len().min(36)];
        sub[n] = b' ';
        sub[n + 1..n + 1 + v.len()].copy_from_slice(v);
        n += 1 + v.len();
    }
    put_center(splash_row() + 4, dim(), &sub[..n], BG); // the build: which kernel is this, at a glance
    footer(b"", b"");
}

/// One centred status line under the logo (ignored while the log or the SSH terminal owns the screen).
pub fn status(s: &str) {
    if unsafe { VGA_LOG } || !splash_shown() {
        return;
    }
    put_center(status_row(), TEXT, s.as_bytes(), BG);
    let x = (cols() - s.len().min(cols())) / 2;
    if x >= 2 {
        // A dot: green when ready, red while recording (the corner REC stays for the other screens).
        put(status_row(), x - 2, if s == "ready" { OK } else if s.starts_with("recording") { REC_DOT } else { dim() }, &[0x07]);
    }
}

static mut MENU_HITS: [(usize, usize, u8); 8] = [(0, 0, 0); 8];
static mut CHOOSER: Option<(usize, usize)> = None; // first row, rows: the host chooser's click targets

/// Host chooser rows under the logo (replacing the status line and the key menu); `sel` highlighted.
pub fn chooser(items: &[&str], sel: usize) {
    chooser_titled(b"SSH: choose a host", b"1-8 or click: connect    arrows + Enter    Esc: back", items, sel);
}

/// The chooser with its own title and hint line (the WiFi screen uses it too).
pub fn chooser_titled(title: &[u8], hint: &[u8], items: &[&str], sel: usize) {
    if unsafe { VGA_LOG } || !splash_shown() {
        return;
    }
    let first = status_row();
    for r in status_row() - 1..rows() - 2 {
        put(r, 0, BG, &[b' '; MAXW][..cols()]);
    }
    put_center(status_row() - 1, dim(), title, BG);
    let w = items.iter().map(|s| s.len()).max().unwrap_or(0);
    let x = cols().saturating_sub(w) / 2;
    for (i, it) in items.iter().enumerate() {
        put(first + i, x, if i == sel { BAR } else { TEXT }, it.as_bytes());
    }
    put_center(rows() - 3, dim(), hint, BG);
    unsafe {
        *core::ptr::addr_of_mut!(MENU_HITS) = [(0, 0, 0); 8];
        CHOOSER = Some((first, items.len()));
    }
}

/// The menu key under a click at cell (col, row) on the splash, if any.
pub fn menu_hit(col: usize, row: usize) -> Option<u8> {
    if unsafe { VGA_LOG } || !splash_shown() {
        return None;
    }
    if let Some((first, n)) = unsafe { CHOOSER } {
        return (first..first + n).contains(&row).then(|| b'1' + (row - first) as u8);
    }
    if row != status_row() + 4 {
        return None;
    }
    unsafe { (*core::ptr::addr_of!(MENU_HITS)).iter().find(|&&(a, b, k)| k != 0 && (a..b).contains(&col)).map(|h| h.2) }
}

/// Key hints under the status line: each key as a light chip, then its label.
pub fn menu(items: &[(&str, &str)]) {
    if unsafe { VGA_LOG } || !splash_shown() {
        return;
    }
    let row = status_row() + 4;
    let w: usize = items.iter().map(|(k, l)| k.len() + 2 + 1 + l.len()).sum::<usize>() + 3 * items.len().saturating_sub(1);
    put(row, 0, BG, &[b' '; MAXW][..cols()]);
    let mut x = cols().saturating_sub(w) / 2;
    let hits = unsafe { &mut *core::ptr::addr_of_mut!(MENU_HITS) };
    *hits = [(0, 0, 0); 8];
    for (n, (k, l)) in items.iter().enumerate() {
        if n < 8 {
            hits[n] = (x, x + k.len() + 3 + l.len(), k.as_bytes()[0]);
        }
        put(row, x + 1, KEY, k.as_bytes());
        x += k.len() + 2;
        put(row, x + 1, TEXT, l.as_bytes());
        x += 1 + l.len() + 3;
    }
}

static mut CORNER: Option<[u8; 5]> = None; // overlay text in the top-right corner (REC), drawn over any screen

fn corner_cell(i: usize) -> Option<u16> {
    let c = unsafe { (*core::ptr::addr_of!(CORNER))? };
    let at = cols().checked_sub(5)?;
    (at..at + 5).contains(&i).then(|| if is_fb() { 0x4000 } else { 0x4F00 } | c[i - at] as u16) // fb: dark on red 5.6:1 (white was 3.1:1)
}

/// A red marker in the top-right corner, an overlay like the pointer: the SSH terminal, a font change or the splash
/// cannot erase it while it is set (Codex review). None removes it and shows the real cells again.
pub fn corner(s: Option<&[u8]>) {
    unsafe {
        let mut c = [b' '; 5];
        if let Some(t) = s {
            for (d, &b) in c[1..].iter_mut().zip(t) {
                *d = b;
            }
        }
        CORNER = s.map(|_| c);
        let at = cols() - 5;
        for i in at..at + 5 {
            let v = shown(i, cell_get64(i));
            if (*core::ptr::addr_of!(FB)).is_some() {
                draw(i, v, CURSOR == Some(i));
            } else {
                VGA.add(i).write_volatile(v as u16);
            }
        }
    }
}

/// Footer bar: `left` at the left edge, `right` at the right edge.
pub fn footer(left: &[u8], right: &[u8]) {
    if unsafe { VGA_LOG } || !splash_shown() {
        return;
    }
    put(rows() - 2, 0, BG, &[b' '; MAXW][..cols()]);
    put(rows() - 2, 2, LINE, &[0xC4; MAXW][..cols().saturating_sub(4)]); // inset to the text margins
    put(rows() - 1, 0, BG, &[b' '; MAXW][..cols()]);
    put(rows() - 1, 2, dim(), left);
    let x = cols().saturating_sub(right.len() + 2);
    put(rows() - 1, x, dim(), right);
    if let Some(k) = right.iter().rposition(|&b| b == b' ') {
        put(rows() - 1, x + k + 1, TEXT, &right[k + 1..]); // the clock (last field) stands out
    }
}

/// The splash owns the screen iff its logo is there (the SSH terminal or the log may have drawn over it).
fn splash_shown() -> bool {
    // TITLE_ON: set by splash, cleared by any other full-screen owner (fill) or foreign write in the title area, so a
    // terminal that happens to print a white N there cannot bring the home screen's footer back over it.
    (unsafe { TITLE_ON }) && title_rect().is_some_and(|(r0, c0, _, _)| cell_get(r0 * cols() + c0) == (TITLE | b'N' as u16))
}

/// Raw byte to COM1 (used to mirror the remote shell's output into the serial log).
pub fn serial_raw(b: u8) {
    serial_putc(b);
}

pub fn init() {
    unsafe {
        outb(COM1 + 1, 0x00); // no interrupts
        outb(COM1 + 3, 0x80); // DLAB
        outb(COM1 + 0, 0x01); // divisor 1 = 115200
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03); // 8N1
        outb(COM1 + 2, 0xC7); // FIFO
        outb(COM1 + 4, 0x03);
    }
    splash();
}

fn serial_putc(b: u8) {
    unsafe {
        // Bounded: a wedged or absent UART must never keep a failure path from reaching its 0xCF9 reset.
        for _ in 0..100_000 {
            if inb(COM1 + 5) & 0x20 != 0 {
                outb(COM1, b);
                return;
            }
        }
    }
}

fn vga_putc(b: u8) {
    unsafe {
        if b == b'\n' {
            COL = 0;
            ROW += 1;
        } else {
            cell(ROW * cols() + COL, ATTR | b as u16);
            COL += 1;
            if COL == cols() {
                COL = 0;
                ROW += 1;
            }
        }
        if ROW == rows() {
            for i in 0..cols() * (rows() - 1) {
                cell(i, cell_get(i + cols()));
            }
            for i in 0..cols() {
                cell((rows() - 1) * cols() + i, ATTR | b' ' as u16);
            }
            ROW = rows() - 1;
        }
    }
}

/// RAM copy of everything printed (first 8 KB), so the attended build can ship its log over UDP.
static mut CAP: [u8; 8192] = [0; 8192];
static mut CAP_LEN: usize = 0;

pub fn captured() -> &'static [u8] {
    unsafe { core::slice::from_raw_parts(core::ptr::addr_of!(CAP) as *const u8, CAP_LEN) }
}

static mut AT_LINE_START: bool = true;

pub struct Console;
impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if unsafe { AT_LINE_START } && crate::time::ready() && !s.is_empty() && !s.starts_with(['\r', '\n']) {
            unsafe { AT_LINE_START = false };
            let t = crate::time::now_ms();
            write!(self, "[{:3}.{:03}] ", t / 1000, t % 1000)?;
        }
        for &b in s.as_bytes() {
            if b == b'\n' {
                serial_putc(b'\r');
            }
            serial_putc(b);
            unsafe {
                if CAP_LEN < 8192 {
                    (*core::ptr::addr_of_mut!(CAP))[CAP_LEN] = b;
                    CAP_LEN += 1;
                }
            }
            if unsafe { VGA_LOG } {
                vga_putc(b);
            }
            if b == b'\n' {
                unsafe { AT_LINE_START = true };
            }
        }
        Ok(())
    }
}

#[macro_export]
macro_rules! kprintln {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = writeln!($crate::console::Console, $($arg)*);
    }};
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::console::Console, $($arg)*);
    }};
}

/// The text of screen row `r` as the user sees it (shadow + REC overlay; pointer not marked): bmctl `screen`.
pub fn row_text(r: usize, out: &mut [u8]) -> usize {
    if r >= rows() {
        return 0;
    }
    let n = cols().min(out.len());
    for (c, b) in out[..n].iter_mut().enumerate() {
        let i = r * cols() + c;
        let v = corner_cell(i).unwrap_or_else(|| cell_get(i));
        *b = match (v & 0xFF) as u8 {
            0xDB => b'#', // the logo's full blocks
            x @ 0x20..=0x7E => x,
            0 => b' ',
            _ => b'?',
        };
    }
    n
}

// --- show-and-tell screen (home key D): ssh::demo draws it through these ---
pub const D_TEXT: u16 = TEXT;
pub const D_KEY: u16 = KEY;
pub const D_OK: u16 = OK;
pub fn d_dim() -> u16 {
    dim()
}

/// Blank the screen for the demo (the splash's title overlay goes off with it).
pub fn demo_clear() {
    unsafe {
        VGA_LOG = false;
        CHOOSER = None;
    }
    cursor(None);
    fill(BG);
}

/// Text at (row, col), clipped to the screen.
pub fn text(row: usize, col: usize, attr: u16, s: &[u8]) {
    if row < rows() {
        put(row, col, attr, s);
    }
}

/// Clear `w` cells at (row, col), then write `s`.
pub fn text_field(row: usize, col: usize, w: usize, attr: u16, s: &[u8]) {
    if row < rows() {
        put(row, col, BG, &[b' '; MAXW][..w.min(cols().saturating_sub(col))]);
        put(row, col, attr, &s[..s.len().min(w)]);
    }
}

/// A single-line box (box-drawing glyphs) with its inside cleared.
pub fn frame(r0: usize, c0: usize, r1: usize, c1: usize) {
    for r in r0..=r1.min(rows() - 1) {
        for c in c0..=c1.min(cols() - 1) {
            let ch = match (r == r0, r == r1, c == c0, c == c1) {
                (true, _, true, _) => 0xDA,
                (true, _, _, true) => 0xBF,
                (_, true, true, _) => 0xC0,
                (_, true, _, true) => 0xD9,
                (true, _, _, _) | (_, true, _, _) => 0xC4,
                (_, _, true, _) | (_, _, _, true) => 0xB3,
                _ => b' ',
            };
            cell(r * cols() + c, if ch == b' ' { BG } else { dim() } | ch as u16);
        }
    }
}

/// Paint one cell (the trackpad trail).
pub fn dot(row: usize, col: usize, attr: u16, ch: u8) {
    if row < rows() && col < cols() {
        cell(row * cols() + col, attr | ch as u16);
    }
}

/// One framebuffer pixel (no-op off the framebuffer).
fn px(fb: &Fb, x: u32, y: u32, p: u32) {
    if x >= fb.w || y >= fb.h {
        return;
    }
    let a = fb.addr + y as u64 * fb.pitch as u64 + (x * fb.bytes) as u64;
    unsafe {
        match fb.bytes {
            4 => core::ptr::write_volatile(a as *mut u32, p),
            3 => {
                core::ptr::write_volatile(a as *mut u16, p as u16);
                core::ptr::write_volatile((a + 2) as *mut u8, (p >> 16) as u8);
            }
            _ => core::ptr::write_volatile(a as *mut u16, p as u16),
        }
    }
}

/// `s` drawn `scale` x the font in palette colour `color`, from cell (row, col), over a cleared box `w` cells wide.
/// Pixels only: the cells underneath stay blank, so a later write to one of them (the pointer) erases its slice;
/// the demo redraws every second. Off the framebuffer: plain text.
pub fn big(row: usize, col: usize, scale: usize, color: u8, w: usize, s: &[u8]) {
    let Some((fb, x0, y0)) = (unsafe { *core::ptr::addr_of!(FB) }) else {
        text_field(row, col, w, (color as u16) << 8, s);
        return;
    };
    let f = font();
    let (fg, bg) = (pixel(&fb, PAL[color as usize & 15]), pixel(&fb, PAL[0]));
    let (cx, cy) = (x0 + (col * f.w) as u32, y0 + (row * f.h) as u32);
    let (bw, bh) = ((w * f.w) as u32, (scale * f.h) as u32);
    for y in 0..bh {
        for x in 0..bw {
            px(&fb, cx + x, cy + y, bg);
        }
    }
    for (k, &ch) in s.iter().enumerate() {
        let g = f.glyph(ch);
        let ox = cx + (k * f.w * scale) as u32;
        if ox + (f.w * scale) as u32 > cx + bw {
            break;
        }
        for y in 0..f.h * scale {
            let bits = g[y / scale];
            for x in 0..f.w * scale {
                if bits >> (f.w - 1 - x / scale) & 1 != 0 {
                    px(&fb, ox + x as u32, cy + y as u32, fg);
                }
            }
        }
    }
}

/// A horizontal meter in cell row `row`: `w` cells wide, filled `permille`/1000 in colour `color`, the rest dim.
pub fn meter(row: usize, col: usize, w: usize, permille: u32, color: u8) {
    let Some((fb, x0, y0)) = (unsafe { *core::ptr::addr_of!(FB) }) else {
        let n = (w as u32 * permille.min(1000) / 1000) as usize;
        for c in 0..w {
            dot(row, col + c, if c < n { (color as u16) << 8 } else { dim() }, if c < n { 0xDB } else { 0xB0 });
        }
        return;
    };
    let f = font();
    let (cx, cy) = (x0 + (col * f.w) as u32, y0 + (row * f.h) as u32 + f.h as u32 / 3);
    let bw = (w * f.w) as u32;
    let fill = bw * permille.min(1000) / 1000;
    let (on, off, bg) = (pixel(&fb, PAL[color as usize & 15]), pixel(&fb, 0x21262D), pixel(&fb, PAL[0]));
    for y in 0..f.h as u32 / 3 {
        for x in 0..bw {
            px(&fb, cx + x, cy + y, if x < fill { on } else { off });
        }
    }
    let _ = bg;
}

/// The framebuffer (gfx.rs draws straight into it) and an RGB colour in its pixel format.
pub fn fb_info() -> Option<Fb> {
    unsafe { (*core::ptr::addr_of!(FB)).map(|(f, _, _)| f) }
}
pub fn rgb(fb: &Fb, c: u32) -> u32 {
    pixel(fb, c)
}

pub fn fb_cell_h() -> usize {
    font().h
}

/// The repository QR code at cell (row, col), `scale` pixels per module, with the 4-module white quiet zone.
/// Returns its size in pixels (0 off the framebuffer).
pub fn qr(row: usize, col: usize, scale: u32) -> u32 {
    let Some((fb, x0, y0)) = (unsafe { *core::ptr::addr_of!(FB) }) else { return 0 };
    let f = font();
    let (cx, cy) = (x0 + (col * f.w) as u32, y0 + (row * f.h) as u32);
    let n = crate::qr::N as u32;
    let side = (n + 8) * scale;
    let (white, black) = (pixel(&fb, 0xFFFFFF), pixel(&fb, 0x000000));
    for y in 0..side {
        for x in 0..side {
            let (mx, my) = ((x / scale) as i32 - 4, (y / scale) as i32 - 4);
            let on = mx >= 0 && my >= 0 && (mx as u32) < n && (my as u32) < n
                && crate::qr::ROWS[my as usize] >> (127 - mx) & 1 != 0;
            px(&fb, cx + x, cy + y, if on { black } else { white });
        }
    }
    side
}

/// After the graphics screen painted pixels behind the cells' backs: clear the framebuffer and forget the shadow, so
/// the next screen redraws every cell (a cell write is otherwise skipped when its text did not change).
pub fn wipe() {
    let Some((fb, _, _)) = (unsafe { *core::ptr::addr_of!(FB) }) else { return };
    let bg = pixel(&fb, PAL[0]);
    for y in 0..fb.h {
        for x in 0..fb.w {
            px(&fb, x, y, bg);
        }
    }
    unsafe { (*core::ptr::addr_of_mut!(SHADOW)).fill(u64::MAX) };
}

/// The show screens (demo, Try-it, graphics) are laid out for the 12x24 font: switch to it, returning the font to
/// restore afterwards with `restore_font`.
pub fn big_font() -> Option<&'static crate::font::Font> {
    unsafe {
        (*core::ptr::addr_of!(FB))?;
        let old = FONT;
        if !core::ptr::eq(old, &crate::font::F12X24) {
            set_font(&crate::font::F12X24);
        }
        Some(old)
    }
}

pub fn restore_font(f: Option<&'static crate::font::Font>) {
    if let Some(f) = f {
        if !core::ptr::eq(f, unsafe { FONT }) {
            set_font(f);
        }
    }
}

pub fn font_wh() -> (usize, usize) {
    (font().w, font().h)
}

pub fn fb_is_wc() -> bool {
    unsafe { FB_WC }
}
