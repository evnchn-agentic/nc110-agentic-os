//! VGA text terminal (80x25 at 0xB8000), the xterm subset that ncurses uses with TERM=xterm (htop, vi):
//! CR LF BS TAB BEL SO SI, deferred autowrap (DECAWM), scroll region (CSI r) with LF/IND/RI/S/T/L/M inside it,
//! cursor CSI A B C D E F G ` d e a H f Z, erase J K X with the current background (bce), P @ (and IRM 4h),
//! REP (CSI b), SGR 0/1/2/22/7/27/30-37/39/40-47/49/90-97/100-107 (+38/48;5;n) -> VGA attributes (16 fg and,
//! with the blink bit turned off by `init`, 16 bg colours),
//! ESC 7/8 + CSI s/u + ?1048, alternate screen ?47/?1047/?1049, ?25 cursor, ?1 DECCKM (read by kbd),
//! DEC line drawing (ESC ( 0 / ESC ) 0 + SO/SI) -> CP437 box characters, DECSTR (CSI ! p), ESC c.
//! OSC/DCS/APC/PM/SOS strings are swallowed. UTF-8 is decoded: CP437 glyph where one exists, else '?';
//! East Asian wide characters take 2 cells ("??"), combining marks take none. Replies (DSR 5n/6n, DA1 CSI c,
//! DA2 CSI > c) queue in `take_reply` for the caller to send back over the channel.
//! The grid lives in RAM (`Term`); `flush` copies it to VGA memory and moves the hardware cursor once per batch.
//! Host tests: `sh tools/test-term.sh`.

/// Largest grid (6x12 font on 1024x600 = 170x50); the live size is `Term::w` x `Term::h` (80x25 on VGA text).
pub const MAXW: usize = 170;
pub const MAXH: usize = 50;
const MAXN: usize = MAXW * MAXH;
const DEF_FG: u8 = 7; // light grey on black
/// A cell: CP437 char (bits 0-7), VGA attribute (8-15), exact foreground RGB (16-39) and background RGB (40-63);
/// an RGB of 0 means "the palette colour of the attribute" (true black is stored as 0x010101).
pub type Cell = u64;
const BLANK: Cell = 0x0700 | b' ' as Cell;
const ANSI_TO_VGA: [u8; 8] = [0, 4, 2, 6, 1, 5, 3, 7]; // VGA swaps the red and blue bits

#[derive(Clone, Copy, PartialEq)]
enum St {
    Ground,
    Esc,
    Charset(u8), // ESC ( or ESC ): one designator byte follows
    EscSkip,     // ESC # % * + ...: one byte follows, ignored
    Csi,
    Str,    // OSC / DCS / APC / PM / SOS: swallowed until BEL or ST
    StrEsc, // ESC seen inside a string
}

#[derive(Clone, Copy)]
struct Saved {
    row: usize,
    col: usize,
    wrap: bool,
    fg: u8,
    bg: u8,
    frgb: u32,
    brgb: u32,
    bold: bool,
    rev: bool,
    g: [bool; 2],
    gl: usize,
}
const SAVED0: Saved = Saved { row: 0, col: 0, wrap: false, fg: DEF_FG, bg: 0, frgb: 0, brgb: 0, bold: false, rev: false, g: [false; 2], gl: 0 };

pub struct Term {
    cells: [Cell; MAXN],
    other: [Cell; MAXN], // the inactive screen (main while the alternate one is shown)
    pub w: usize,
    pub h: usize,
    alt: bool,
    row: usize,
    col: usize,
    wrap: bool, // deferred wrap: last column written, next printable goes to the next line
    st: St,
    p: [u16; 16],
    np: usize,
    private: bool,
    lead: u8,  // CSI private marker byte (? > < =), 0 = none
    inter: u8, // CSI intermediate byte (0 = none)
    frgb: u32, // exact foreground (256-colour / truecolour SGR), 0 = the palette colour of `fg`
    brgb: u32, // exact background, 0 = the palette colour of `bg`
    fg: u8,    // VGA colour 0..15
    bg: u8,    // VGA colour 0..15 (attribute bit 7 = bright background once `init` turns blink off)
    bold: bool,
    rev: bool,
    g: [bool; 2], // G0/G1 designated as DEC special graphics
    gl: usize,    // 0 = G0 (SI), 1 = G1 (SO)
    top: usize,
    bot: usize,
    saved: [Saved; 2], // per screen (main, alternate), as xterm does
    last: u32, // last printed character (code point), for REP
    utf: u32,  // UTF-8 decoder: code point so far
    need: u8,  // continuation bytes still expected
    min: u32,  // smallest code point the current sequence length may encode (overlong check)
    reply: [u8; 64],
    nreply: usize,
    pub counts: [u32; 5], // replies sent: DSR 6n, DSR 5n, DA1, DA2, dropped (reply buffer full)
    cursor_on: bool,
    app_cursor: bool,
    insert: bool,
    autowrap: bool,
    mouse: u16,     // DECSET ?1000 (buttons), ?1002 (+drag), ?1003 (+any motion); 0 = off
    mouse_sgr: bool, // ?1006: SGR encoding (ESC [ < b ; x ; y M/m)
}

impl Term {
    fn n(&self) -> usize {
        self.w * self.h
    }
    /// New grid size (clamped to the maximum): clears both screens, homes the cursor, resets the scroll region.
    pub fn resize(&mut self, w: usize, h: usize) {
        (self.w, self.h) = (w.clamp(20, MAXW), h.clamp(5, MAXH));
        self.reset();
    }
    /// New grid size in a running session (F11): both screens cleared for the app's SIGWINCH redraw, but the modes it
    /// set (alternate screen, cursor keys, mouse tracking, wrap/insert, colours) and the parser state are kept.
    pub fn regrid(&mut self, w: usize, h: usize) {
        (self.w, self.h) = (w.clamp(20, MAXW), h.clamp(5, MAXH));
        self.fill(0, self.n());
        self.other.fill(BLANK);
        (self.row, self.col, self.wrap) = (self.row.min(self.h - 1), self.col.min(self.w - 1), false);
        (self.top, self.bot) = (0, self.h - 1);
    }
    pub const fn new() -> Term {
        Term {
            cells: [BLANK; MAXN],
            other: [BLANK; MAXN],
            w: 80,
            h: 25,
            alt: false,
            row: 0,
            col: 0,
            wrap: false,
            st: St::Ground,
            p: [0; 16],
            np: 0,
            private: false,
            lead: 0,
            inter: 0,
            frgb: 0,
            brgb: 0,
            fg: DEF_FG,
            bg: 0,
            bold: false,
            rev: false,
            g: [false; 2],
            gl: 0,
            top: 0,
            bot: 24,
            saved: [SAVED0; 2],
            last: b' ' as u32,
            utf: 0,
            need: 0,
            min: 0,
            reply: [0; 64],
            nreply: 0,
            counts: [0; 5],
            cursor_on: true,
            app_cursor: false,
            insert: false,
            autowrap: true,
            mouse: 0,
            mouse_sgr: false,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn cell(&self, r: usize, c: usize) -> u16 {
        self.cells[r * self.w + c] as u16
    }
    /// The cell with its exact colours (see `Cell`).
    pub fn cell_full(&self, r: usize, c: usize) -> Cell {
        self.cells[r * self.w + c]
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.col.min(self.w - 1))
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn cursor_visible(&self) -> bool {
        self.cursor_on
    }
    pub fn app_cursor(&self) -> bool {
        self.app_cursor
    }
    pub fn mouse(&self) -> (u16, bool) {
        (self.mouse, self.mouse_sgr)
    }
    /// Move queued replies (terminal -> host) into `out`; returns how many bytes were moved.
    pub fn take_reply(&mut self, out: &mut [u8]) -> usize {
        let n = self.nreply.min(out.len());
        out[..n].copy_from_slice(&self.reply[..n]);
        self.reply.copy_within(n..self.nreply, 0);
        self.nreply -= n;
        n
    }
    fn send(&mut self, kind: usize, s: &[u8]) {
        if self.nreply + s.len() > self.reply.len() {
            self.counts[4] += 1; // a reply is all-or-nothing: a truncated CPR would be worse than none
            return;
        }
        self.reply[self.nreply..self.nreply + s.len()].copy_from_slice(s);
        self.nreply += s.len();
        self.counts[kind] += 1;
    }

    fn attr(&self) -> Cell {
        let (mut f, mut b) = (self.fg | if self.bold { 8 } else { 0 }, self.bg);
        let (mut fr, mut br) = (self.frgb, self.brgb);
        if self.rev {
            (f, b) = (b, f);
            (fr, br) = (br, fr);
        }
        ((b as Cell) << 12) | ((f as Cell) << 8) | (fr as Cell) << 16 | (br as Cell) << 40
    }
    fn blank(&self) -> Cell {
        self.attr() | b' ' as Cell // bce: erased cells take the current background
    }
    fn fill(&mut self, from: usize, to: usize) {
        let b = self.blank();
        let n = self.n();
        self.cells[from.min(n)..to.min(n)].fill(b);
    }
    /// Scroll rows top..=bot by n, up (content moves up, blank rows at the bottom) or down.
    fn scroll(&mut self, top: usize, bot: usize, n: usize, up: bool) {
        let n = n.min(bot + 1 - top);
        if up {
            self.cells.copy_within((top + n) * self.w..(bot + 1) * self.w, top * self.w);
            self.fill((bot + 1 - n) * self.w, (bot + 1) * self.w);
        } else {
            self.cells.copy_within(top * self.w..(bot + 1 - n) * self.w, (top + n) * self.w);
            self.fill(top * self.w, (top + n) * self.w);
        }
    }
    fn lf(&mut self) {
        if self.row == self.bot {
            self.scroll(self.top, self.bot, 1, true);
        } else if self.row < self.h - 1 {
            self.row += 1;
        }
    }
    fn ri(&mut self) {
        if self.row == self.top {
            self.scroll(self.top, self.bot, 1, false);
        } else if self.row > 0 {
            self.row -= 1;
        }
    }
    fn save(&mut self) {
        self.saved[self.alt as usize] = Saved { row: self.row, col: self.col, wrap: self.wrap, fg: self.fg, bg: self.bg, frgb: self.frgb, brgb: self.brgb, bold: self.bold, rev: self.rev, g: self.g, gl: self.gl };
    }
    fn restore(&mut self) {
        let s = self.saved[self.alt as usize];
        (self.row, self.col, self.wrap, self.fg, self.bg, self.bold, self.rev, self.g, self.gl) = (s.row, s.col, s.wrap, s.fg, s.bg, s.bold, s.rev, s.g, s.gl);
        (self.frgb, self.brgb) = (s.frgb, s.brgb);
    }
    fn set_alt(&mut self, on: bool, clear: bool) {
        if on != self.alt {
            core::mem::swap(&mut self.cells, &mut self.other);
            self.alt = on;
            if on && clear {
                self.fill(0, self.n());
            }
        }
    }
    /// DECSTR soft reset: modes, attributes, margins and charsets; the screen is kept.
    fn soft_reset(&mut self) {
        (self.fg, self.bg, self.bold, self.rev, self.g, self.gl, self.frgb, self.brgb) = (DEF_FG, 0, false, false, [false; 2], 0, 0, 0);
        (self.top, self.bot, self.insert, self.autowrap, self.app_cursor, self.cursor_on) = (0, self.h - 1, false, true, false, true);
        self.saved = [SAVED0; 2];
    }
    /// RIS (ESC c), in place (no 8 KiB temporary on the kernel stack): main screen, cleared, cursor home.
    pub fn reset(&mut self) {
        self.set_alt(false, false);
        self.soft_reset();
        self.fill(0, self.n());
        self.other.fill(BLANK);
        (self.row, self.col, self.wrap, self.st, self.last, self.need) = (0, 0, false, St::Ground, b' ' as u32, 0);
        (self.mouse, self.mouse_sgr) = (0, false); // a new session starts with mouse tracking off
    }

    /// Print one character (code point): DEC graphics for ASCII when G0/G1 says so, else CP437 or '?';
    /// wide characters take two cells, zero-width ones none.
    fn print(&mut self, cp: u32) {
        let w = width(cp);
        if w == 0 {
            return;
        }
        self.last = cp;
        let ch = if cp < 0x80 && self.g[self.gl] && (0x5F..=0x7E).contains(&(cp as u8)) {
            DEC_GRAPHICS[(cp - 0x5F) as usize]
        } else {
            cp437(cp)
        };
        if w == 2 {
            if self.col >= self.w - 1 && !self.wrap && !self.autowrap {
                return; // no room and no wrap: xterm drops the character rather than overwrite the last cell
            }
            if self.wrap || self.col >= self.w - 1 {
                self.col = 0; // no room for both halves: wrap first, as xterm does
                self.lf();
                self.wrap = false;
            }
            self.put(b'?');
            self.put(b'?');
        } else {
            self.put(ch);
        }
    }
    fn put(&mut self, ch: u8) {
        if self.wrap {
            self.col = 0;
            self.lf();
            self.wrap = false;
        }
        let s = self.row * self.w;
        if self.insert {
            self.cells.copy_within(s + self.col..s + self.w - 1, s + self.col + 1);
        }
        self.cells[s + self.col] = self.attr() | ch as Cell;
        if self.col == self.w - 1 {
            self.wrap = self.autowrap;
        } else {
            self.col += 1;
        }
    }

    fn arg(&self, i: usize, def: u16) -> usize {
        let v = if i < self.np { self.p[i] } else { 0 };
        (if v == 0 { def } else { v }) as usize
    }

    fn sgr(&mut self) {
        let np = self.np.max(1).min(self.p.len());
        let mut i = 0;
        while i < np {
            let v = self.p[i];
            match v {
                0 => (self.fg, self.bg, self.bold, self.rev, self.frgb, self.brgb) = (DEF_FG, 0, false, false, 0, 0),
                1 => self.bold = true,
                22 => self.bold = false,
                7 => self.rev = true,
                27 => self.rev = false,
                30..=37 => (self.fg, self.frgb) = (ANSI_TO_VGA[(v - 30) as usize], 0),
                39 => (self.fg, self.frgb) = (DEF_FG, 0),
                40..=47 => (self.bg, self.brgb) = (ANSI_TO_VGA[(v - 40) as usize], 0),
                49 => (self.bg, self.brgb) = (0, 0),
                90..=97 => (self.fg, self.frgb) = (ANSI_TO_VGA[(v - 90) as usize] | 8, 0),
                100..=107 => (self.bg, self.brgb) = (ANSI_TO_VGA[(v - 100) as usize] | 8, 0),
                38 | 48 => {
                    let sub = if i + 1 < np { self.p[i + 1] } else { 0 };
                    // 256-colour and truecolour: the nearest of the 16 the screen has (modern TUIs such as herdr)
                    // the exact colour for the framebuffer, and the nearest of 16 for VGA text and the attribute
                    let n = if sub == 5 && i + 2 < np {
                        i += 2;
                        let k = self.p[i] as u32;
                        Some((xterm256_to_16(k), if k < 16 { 0 } else { xterm256_rgb(k) }))
                    } else if sub == 2 && i + 4 < np {
                        i += 4;
                        let (r, g, b) = ((self.p[i - 2] as u32).min(255), (self.p[i - 1] as u32).min(255), (self.p[i] as u32).min(255));
                        Some((nearest16(r, g, b), nonzero(r << 16 | g << 8 | b)))
                    } else {
                        None
                    };
                    if let Some((n, rgb)) = n {
                        let c = ANSI_TO_VGA[(n & 7) as usize] | if n >= 8 { 8 } else { 0 };
                        if v == 38 { (self.fg, self.frgb) = (c, rgb) } else { (self.bg, self.brgb) = (c, rgb) }
                    }
                }
                _ => {} // 2 dim, 4 underline, 5 blink, 8 invisible, 3/23 italic, ...: no VGA equivalent
            }
            i += 1;
        }
    }

    fn mode(&mut self, on: bool) {
        for i in 0..self.np.min(self.p.len()) {
            match (self.private, self.p[i]) {
                (false, 4) => self.insert = on,
                (true, 1) => self.app_cursor = on,
                (true, 7) => {
                    self.autowrap = on;
                    self.wrap = false;
                }
                (true, 25) => self.cursor_on = on,
                (true, m @ (1000 | 1002 | 1003)) => self.mouse = if on { m } else { 0 },
                (true, 1006) => self.mouse_sgr = on,
                (true, 47) => self.set_alt(on, false),
                (true, 1047) => self.set_alt(on, true),
                (true, 1048) => {
                    if on {
                        self.save()
                    } else {
                        self.restore()
                    }
                }
                (true, 1049) => {
                    if on {
                        self.save();
                        self.set_alt(true, true);
                    } else {
                        self.set_alt(false, false);
                        self.restore();
                    }
                }
                _ => {} // ?12 blink, ?1005/?1015 mouse encodings, ?2004 paste, 20 LNM, ...: not modelled
            }
        }
    }

    fn csi(&mut self, f: u8) {
        if f == b'c' && self.inter == 0 && self.arg(0, 0) == 0 {
            match self.lead {
                0 => self.send(2, b"\x1b[?6c"),         // DA1: VT102
                b'>' => self.send(3, b"\x1b[>0;10;0c"), // DA2: VT100-class, version 10 (low: vim enables no xterm extras)
                _ => {}
            }
            return;
        }
        if f == b'n' && self.inter == 0 && self.lead == 0 {
            match self.arg(0, 0) {
                5 => self.send(1, b"\x1b[0n"),
                6 => {
                    // CPR, 1-based; a pending wrap reports the last column, as xterm does
                    let (mut b, mut n) = ([0u8; 8], 2);
                    b[..2].copy_from_slice(b"\x1b[");
                    for (i, v) in [self.row + 1, self.col.min(self.w - 1) + 1].into_iter().enumerate() {
                        if v >= 10 {
                            b[n] = b'0' + (v / 10) as u8;
                            n += 1;
                        }
                        b[n] = b'0' + (v % 10) as u8;
                        b[n + 1] = if i == 0 { b';' } else { b'R' };
                        n += 2;
                    }
                    self.send(0, &b[..n]);
                }
                _ => {}
            }
            return;
        }
        if self.inter != 0 {
            if self.inter == b'!' && f == b'p' {
                self.soft_reset();
            }
            return; // CSI SP q (cursor style), CSI " q, ...: ignored
        }
        if f == b'h' || f == b'l' {
            return self.mode(f == b'h');
        }
        if self.private {
            return; // CSI ? s/r (save/restore modes), CSI > c, CSI > 4;2 m, CSI ? u, ...
        }
        let n = self.arg(0, 1);
        let (row, col) = (self.row, self.col);
        match f {
            b'm' => return self.sgr(),
            b'b' => {
                for _ in 0..n.min(self.n()) {
                    self.print(self.last);
                }
                return;
            }
            b'A' | b'F' => self.row = if row >= self.top { row.saturating_sub(n).max(self.top) } else { row.saturating_sub(n) },
            b'B' | b'e' | b'E' => self.row = if row <= self.bot { (row + n).min(self.bot) } else { (row + n).min(self.h - 1) },
            b'C' | b'a' => self.col = (col + n).min(self.w - 1),
            b'D' => self.col = col.min(self.w - 1).saturating_sub(n),
            b'G' | b'`' => self.col = (n - 1).min(self.w - 1),
            b'd' => self.row = (n - 1).min(self.h - 1),
            b'H' | b'f' => {
                self.row = (self.arg(0, 1) - 1).min(self.h - 1);
                self.col = (self.arg(1, 1) - 1).min(self.w - 1);
            }
            b'Z' => {
                for _ in 0..n.min(self.w) {
                    self.col = self.col.min(self.w - 1).saturating_sub(1) & !7;
                }
            }
            b'J' => {
                let cur = row * self.w + col.min(self.w - 1);
                match self.arg(0, 0) {
                    0 => self.fill(cur, self.n()),
                    1 => self.fill(0, cur + 1),
                    _ => self.fill(0, self.n()),
                }
            }
            b'K' => {
                let (s, c) = (row * self.w, col.min(self.w - 1));
                match self.arg(0, 0) {
                    0 => self.fill(s + c, s + self.w),
                    1 => self.fill(s, s + c + 1),
                    _ => self.fill(s, s + self.w),
                }
            }
            b'P' | b'@' | b'X' => {
                let (s, c) = (row * self.w, col.min(self.w - 1));
                let n = n.min(self.w - c);
                match f {
                    b'P' => {
                        self.cells.copy_within(s + c + n..s + self.w, s + c);
                        self.fill(s + self.w - n, s + self.w);
                    }
                    b'@' => {
                        self.cells.copy_within(s + c..s + self.w - n, s + c + n);
                        self.fill(s + c, s + c + n);
                    }
                    _ => self.fill(s + c, s + c + n),
                }
            }
            b'L' | b'M' => {
                if row >= self.top && row <= self.bot {
                    self.scroll(row, self.bot, n, f == b'M');
                    self.col = 0;
                }
            }
            b'S' => self.scroll(self.top, self.bot, n, true),
            b'T' if self.np <= 1 => self.scroll(self.top, self.bot, n, false),
            b'r' => {
                let (t, b) = (self.arg(0, 1) - 1, self.arg(1, self.h as u16).min(self.h) - 1);
                if t < b {
                    (self.top, self.bot, self.row, self.col) = (t, b, 0, 0);
                }
            }
            b's' => self.save(),
            b'u' => self.restore(),
            _ => return, // 't' (window ops), 'g' (tabs), ...: ignored, keep a pending wrap
        }
        if f != b'u' {
            self.wrap = false;
        }
        if matches!(f, b'E' | b'F') {
            self.col = 0;
        }
    }

    pub fn feed(&mut self, b: u8) {
        if self.need > 0 {
            if b & 0xC0 == 0x80 {
                self.utf = (self.utf << 6) | (b & 0x3F) as u32;
                self.need -= 1;
                if self.need == 0 {
                    let cp = self.utf;
                    let ok = cp >= self.min && !(0xD800..=0xDFFF).contains(&cp) && cp <= 0x10FFFF;
                    self.print(if ok { cp } else { b'?' as u32 });
                }
                return;
            }
            self.need = 0; // sequence cut short (by a control, ESC, or a new lead byte): one '?', then this byte
            self.print(b'?' as u32);
        }
        match self.st {
            St::Ground => match b {
                0x1B => self.st = St::Esc,
                b'\r' => {
                    self.col = 0;
                    self.wrap = false;
                }
                b'\n' | 0x0B | 0x0C => {
                    self.lf();
                    self.wrap = false;
                }
                0x08 => {
                    self.col = self.col.min(self.w - 1).saturating_sub(1);
                    self.wrap = false;
                }
                b'\t' => self.col = ((self.col + 8) & !7).min(self.w - 1),
                0x0E => self.gl = 1,
                0x0F => self.gl = 0,
                0xC2..=0xF4 => {
                    let (need, min, mask) = match b {
                        0xC2..=0xDF => (1, 0x80, 0x1F),
                        0xE0..=0xEF => (2, 0x800, 0x0F),
                        _ => (3, 0x10000, 0x07),
                    };
                    (self.utf, self.need, self.min) = ((b & mask) as u32, need, min);
                }
                0x80..=0xFF => self.print(b'?' as u32), // stray continuation or invalid lead byte (U+FFFD)
                _ if b < 0x20 || b == 0x7F => {} // BEL and other controls
                _ => self.print(b as u32),
            },
            St::Esc => {
                self.st = St::Ground;
                match b {
                    b'[' => {
                        self.st = St::Csi;
                        (self.p, self.np, self.private, self.lead, self.inter) = ([0; 16], 0, false, 0, 0);
                    }
                    b']' | b'P' | b'_' | b'^' | b'X' => self.st = St::Str,
                    b'(' => self.st = St::Charset(0),
                    b')' => self.st = St::Charset(1),
                    b'*' | b'+' | b'-' | b'.' | b'/' | b'#' | b'%' | b' ' => self.st = St::EscSkip,
                    b'7' => self.save(),
                    b'8' => self.restore(),
                    b'D' => self.lf(),
                    b'E' => {
                        self.col = 0;
                        self.lf();
                    }
                    b'M' => self.ri(),
                    b'c' => self.reset(),
                    0x1B => self.st = St::Esc,
                    _ => {} // '=', '>' (keypad modes), 'H' (tab set), ...
                }
                if matches!(b, b'D' | b'E' | b'M') {
                    self.wrap = false;
                }
            }
            St::Charset(i) => {
                self.g[i as usize] = b == b'0';
                self.st = St::Ground;
            }
            St::EscSkip => self.st = St::Ground,
            St::Csi => match b {
                b'0'..=b'9' => {
                    if self.np == 0 {
                        self.np = 1;
                    }
                    if let Some(v) = self.p.get_mut(self.np - 1) {
                        *v = v.saturating_mul(10).saturating_add((b - b'0') as u16);
                    }
                }
                b';' => self.np = self.np.max(1) + 1,
                b':' => self.inter = b':', // colon sub-parameters (38:2::r:g:b): sequence ignored, not misparsed
                b'?' | b'>' | b'<' | b'=' => (self.private, self.lead) = (true, b),
                0x20..=0x2F => self.inter = b,
                0x40..=0x7E => {
                    self.st = St::Ground;
                    self.csi(b);
                }
                0x1B => self.st = St::Esc,
                _ => {} // controls inside CSI: ignored
            },
            St::Str => match b {
                0x07 => self.st = St::Ground,
                0x1B => self.st = St::StrEsc,
                _ => {}
            },
            St::StrEsc => self.st = if b == b'\\' { St::Ground } else { St::Str },
        }
    }
}

/// DEC special graphics 0x5F..0x7E -> CP437. Glyphs CP437 lacks fall back to the nearest one.
const DEC_GRAPHICS: [u8; 32] = [
    b' ', // _ blank
    0x04, // ` diamond
    0xB1, // a checkerboard
    b' ', // b HT symbol
    b' ', // c FF symbol
    b' ', // d CR symbol
    b' ', // e LF symbol
    0xF8, // f degree
    0xF1, // g plus/minus
    b' ', // h NL symbol
    b' ', // i VT symbol
    0xD9, // j lower right corner
    0xBF, // k upper right corner
    0xDA, // l upper left corner
    0xC0, // m lower left corner
    0xC5, // n crossing lines
    0xC4, // o scan line 1
    0xC4, // p scan line 3
    0xC4, // q horizontal line (scan line 5)
    0xC4, // r scan line 7
    b'_', // s scan line 9
    0xC3, // t left tee
    0xB4, // u right tee
    0xC1, // v bottom tee
    0xC2, // w top tee
    0xB3, // x vertical line
    0xF3, // y less-or-equal
    0xF2, // z greater-or-equal
    0xE3, // { pi
    b'#', // | not-equal (none in CP437)
    0x9C, // } pound sign
    0xFA, // ~ centred dot
];

/// Unicode -> CP437 for every glyph the VGA font has outside ASCII (0x01-0x1F, 0x7F, 0x80-0xFF), sorted by code point.
/// No fallbacks: a character CP437 lacks (e.g. U+00D7 multiplication sign) shows as '?'.
const CP437: [(u16, u8); 227] = [
    (0x00A0, 0xFF),
    (0x00A1, 0xAD),
    (0x00A2, 0x9B),
    (0x00A3, 0x9C),
    (0x00A5, 0x9D),
    (0x00A7, 0x15),
    (0x00AA, 0xA6),
    (0x00AB, 0xAE),
    (0x00AC, 0xAA),
    (0x00B0, 0xF8),
    (0x00B1, 0xF1),
    (0x00B2, 0xFD),
    (0x00B5, 0xE6),
    (0x00B6, 0x14),
    (0x00B7, 0xFA),
    (0x00BA, 0xA7),
    (0x00BB, 0xAF),
    (0x00BC, 0xAC),
    (0x00BD, 0xAB),
    (0x00BF, 0xA8),
    (0x00C4, 0x8E),
    (0x00C5, 0x8F),
    (0x00C6, 0x92),
    (0x00C7, 0x80),
    (0x00C9, 0x90),
    (0x00D1, 0xA5),
    (0x00D6, 0x99),
    (0x00DC, 0x9A),
    (0x00DF, 0xE1),
    (0x00E0, 0x85),
    (0x00E1, 0xA0),
    (0x00E2, 0x83),
    (0x00E4, 0x84),
    (0x00E5, 0x86),
    (0x00E6, 0x91),
    (0x00E7, 0x87),
    (0x00E8, 0x8A),
    (0x00E9, 0x82),
    (0x00EA, 0x88),
    (0x00EB, 0x89),
    (0x00EC, 0x8D),
    (0x00ED, 0xA1),
    (0x00EE, 0x8C),
    (0x00EF, 0x8B),
    (0x00F1, 0xA4),
    (0x00F2, 0x95),
    (0x00F3, 0xA2),
    (0x00F4, 0x93),
    (0x00F6, 0x94),
    (0x00F7, 0xF6),
    (0x00F9, 0x97),
    (0x00FA, 0xA3),
    (0x00FB, 0x96),
    (0x00FC, 0x81),
    (0x00FF, 0x98),
    (0x0192, 0x9F),
    (0x0393, 0xE2),
    (0x0398, 0xE9),
    (0x03A3, 0xE4),
    (0x03A6, 0xE8),
    (0x03A9, 0xEA),
    (0x03B1, 0xE0),
    (0x03B4, 0xEB),
    (0x03B5, 0xEE),
    (0x03C0, 0xE3),
    (0x03C3, 0xE5),
    (0x03C4, 0xE7),
    (0x03C6, 0xED),
    (0x2022, 0x07),
    (0x2026, 0xFA),
    (0x2039, 0xAE),
    (0x203A, 0xAF),
    (0x203C, 0x13),
    (0x207F, 0xFC),
    (0x20A7, 0x9E),
    (0x2190, 0x1B),
    (0x2191, 0x18),
    (0x2192, 0x1A),
    (0x2193, 0x19),
    (0x2194, 0x1D),
    (0x2195, 0x12),
    (0x21A8, 0x17),
    (0x2217, b'*'),
    (0x2219, 0xF9),
    (0x221A, 0xFB),
    (0x221E, 0xEC),
    (0x221F, 0x1C),
    (0x2229, 0xEF),
    (0x2234, b':'),
    (0x2235, b':'),
    (0x2248, 0xF7),
    (0x2261, 0xF0),
    (0x2264, 0xF3),
    (0x2265, 0xF2),
    (0x22EF, 0xFA),
    (0x2302, 0x7F),
    (0x2310, 0xA9),
    (0x2320, 0xF4),
    (0x2321, 0xF5),
    (0x23BF, 0xC0),
    (0x23F4, 0x11),
    (0x23F5, 0x10),
    (0x23F8, 0xBA),
    (0x23F9, 0xFE),
    (0x23FA, 0x07),
    (0x2500, 0xC4),
    (0x2501, 0xC4),
    (0x2502, 0xB3),
    (0x2503, 0xB3),
    (0x250C, 0xDA),
    (0x250F, 0xDA),
    (0x2510, 0xBF),
    (0x2513, 0xBF),
    (0x2514, 0xC0),
    (0x2517, 0xC0),
    (0x2518, 0xD9),
    (0x251B, 0xD9),
    (0x251C, 0xC3),
    (0x2523, 0xC3),
    (0x2524, 0xB4),
    (0x252B, 0xB4),
    (0x252C, 0xC2),
    (0x2533, 0xC2),
    (0x2534, 0xC1),
    (0x253B, 0xC1),
    (0x253C, 0xC5),
    (0x254B, 0xC5),
    (0x2550, 0xCD),
    (0x2551, 0xBA),
    (0x2552, 0xD5),
    (0x2553, 0xD6),
    (0x2554, 0xC9),
    (0x2555, 0xB8),
    (0x2556, 0xB7),
    (0x2557, 0xBB),
    (0x2558, 0xD4),
    (0x2559, 0xD3),
    (0x255A, 0xC8),
    (0x255B, 0xBE),
    (0x255C, 0xBD),
    (0x255D, 0xBC),
    (0x255E, 0xC6),
    (0x255F, 0xC7),
    (0x2560, 0xCC),
    (0x2561, 0xB5),
    (0x2562, 0xB6),
    (0x2563, 0xB9),
    (0x2564, 0xD1),
    (0x2565, 0xD2),
    (0x2566, 0xCB),
    (0x2567, 0xCF),
    (0x2568, 0xD0),
    (0x2569, 0xCA),
    (0x256A, 0xD8),
    (0x256B, 0xD7),
    (0x256C, 0xCE),
    (0x256D, 0xDA),
    (0x256E, 0xBF),
    (0x256F, 0xD9),
    (0x2570, 0xC0),
    (0x2574, 0xC4),
    (0x2575, 0xB3),
    (0x2576, 0xC4),
    (0x2577, 0xB3),
    (0x2578, 0xC4),
    (0x2579, 0xB3),
    (0x257A, 0xC4),
    (0x257B, 0xB3),
    (0x2580, 0xDF),
    (0x2581, b'_'),
    (0x2584, 0xDC),
    (0x2588, 0xDB),
    (0x258C, 0xDD),
    (0x258F, 0xB3),
    (0x2590, 0xDE),
    (0x2591, 0xB0),
    (0x2592, 0xB1),
    (0x2593, 0xB2),
    (0x2594, 0xC4),
    (0x2595, 0xB3),
    (0x25A0, 0xFE),
    (0x25A1, 0xFE),
    (0x25AA, 0xFE),
    (0x25AC, 0x16),
    (0x25B2, 0x1E),
    (0x25B4, 0x1E),
    (0x25B6, 0x10),
    (0x25B8, 0x10),
    (0x25BA, 0x10),
    (0x25BC, 0x1F),
    (0x25BE, 0x1F),
    (0x25C0, 0x11),
    (0x25C2, 0x11),
    (0x25C4, 0x11),
    (0x25C6, 0x04),
    (0x25C7, 0x04),
    (0x25C9, 0x07),
    (0x25CB, 0x09),
    (0x25CF, 0x07),
    (0x25D8, 0x08),
    (0x25D9, 0x0A),
    (0x263A, 0x01),
    (0x263B, 0x02),
    (0x263C, 0x0F),
    (0x2640, 0x0C),
    (0x2642, 0x0B),
    (0x2660, 0x06),
    (0x2663, 0x05),
    (0x2665, 0x03),
    (0x2666, 0x04),
    (0x266A, 0x0D),
    (0x266B, 0x0E),
    (0x26A0, b'!'),
    (0x2713, 0xFB),
    (0x2714, 0xFB),
    (0x2717, b'x'),
    (0x2718, b'x'),
    (0x2722, b'+'),
    (0x2731, b'*'),
    (0x2733, b'*'),
    (0x2736, b'*'),
    (0x273B, b'*'),
    (0x273D, b'*'),
    (0x276F, 0xAF),
    (0x29C9, b'#'),
    (0x2B24, 0x07),
    (0x2E31, 0xFA),
];

/// xterm's default 16 colours (ANSI order) as RGB, for nearest-colour matching.
const ANSI_RGB: [(u32, u32, u32); 16] = [(0, 0, 0), (205, 0, 0), (0, 205, 0), (205, 205, 0), (0, 0, 238), (205, 0, 205), (0, 205, 205),
    (229, 229, 229), (127, 127, 127), (255, 0, 0), (0, 255, 0), (255, 255, 0), (92, 92, 255), (255, 0, 255), (0, 255, 255), (255, 255, 255)];

fn nearest16(r: u32, g: u32, b: u32) -> u8 {
    let d = |&(x, y, z): &(u32, u32, u32)| {
        let (dr, dg, db) = (r as i32 - x as i32, g as i32 - y as i32, b as i32 - z as i32);
        2 * dr * dr + 4 * dg * dg + 3 * db * db
    };
    (0..16).min_by_key(|&i| d(&ANSI_RGB[i])).unwrap_or(7) as u8
}

/// An exact colour as stored in a cell: 0 means "palette", so true black becomes 0x010101 (and nothing else changes:
/// `max(0x010101)` turned every colour without red, such as 0x0000AF, into black).
fn nonzero(v: u32) -> u32 {
    if v == 0 { 0x010101 } else { v }
}

/// The xterm 256-colour index (16-255) as RGB: the 6x6x6 cube and the grey ramp.
fn xterm256_rgb(n: u32) -> u32 {
    let v = if n < 232 {
        let k = n - 16;
        let lv = |v: u32| if v == 0 { 0 } else { 55 + 40 * v };
        lv(k / 36) << 16 | lv(k / 6 % 6) << 8 | lv(k % 6)
    } else {
        let g = 8 + 10 * (n.min(255) - 232);
        g << 16 | g << 8 | g
    };
    nonzero(v)
}

/// The xterm 256-colour index as one of 16: 0-15 as they are, the 6x6x6 cube and the grey ramp by nearest colour.
fn xterm256_to_16(n: u32) -> u8 {
    match n {
        0..=15 => n as u8,
        16..=231 => {
            let k = n - 16;
            let lv = |v: u32| if v == 0 { 0 } else { 55 + 40 * v };
            nearest16(lv(k / 36), lv(k / 6 % 6), lv(k % 6))
        }
        _ => {
            let g = 8 + 10 * (n.min(255) - 232);
            nearest16(g, g, g)
        }
    }
}

/// Code points drawn as '?' (each with a count), for bmctl `term unmapped`: real TUIs (herdr, Claude Code) say which
/// glyphs are still missing. 32 distinct, then counted but not stored.
static mut UNMAPPED: [(u32, u32); 32] = [(0, 0); 32];

fn note_unmapped(cp: u32) {
    let u = unsafe { &mut *core::ptr::addr_of_mut!(UNMAPPED) };
    if let Some(e) = u.iter_mut().find(|e| e.0 == cp || e.0 == 0) {
        (e.0, e.1) = (cp, e.1.saturating_add(1));
    }
}

/// (code point, count) of the glyphs drawn as '?' so far.
pub fn unmapped() -> impl Iterator<Item = (u32, u32)> {
    unsafe { (*core::ptr::addr_of!(UNMAPPED)).iter().copied().filter(|e| e.0 != 0) }
}

fn cp437(cp: u32) -> u8 {
    if (0x20..0x7F).contains(&cp) {
        return cp as u8;
    }
    if (0x2800..=0x28FF).contains(&cp) {
        return if cp == 0x2800 { b' ' } else { 0xF9 }; // braille (spinners, graphs): a middle dot
    }
    match CP437.binary_search_by_key(&cp, |&(u, _)| u as u32) {
        Ok(i) => CP437[i].1,
        Err(_) => {
            note_unmapped(cp);
            b'?'
        }
    }
}

/// Cells a character takes, as wcwidth (and so pyte) counts them: 0 for combining marks, zero-width characters and C1
/// controls, 2 for East Asian Wide/Fullwidth (CJK, Hangul, fullwidth forms, emoji), else 1.
fn width(cp: u32) -> usize {
    let within = |t: &[(u32, u32)]| t.binary_search_by(|&(a, b)| if b < cp { core::cmp::Ordering::Less } else if a > cp { core::cmp::Ordering::Greater } else { core::cmp::Ordering::Equal }).is_ok();
    if cp < 0xA0 {
        (cp < 0x80) as usize // ASCII 1; C1 controls (only reachable as decoded UTF-8) are not printed
    } else if within(&ZERO_WIDTH) {
        0
    } else if within(&WIDE) {
        2
    } else {
        1
    }
}

// GENERATED by tools/gen-width.py from wcwidth 0.9.2 (0xA0 and up): do not edit by hand
const ZERO_WIDTH: [(u32, u32); 342] = [
    (0x300, 0x36F), (0x483, 0x489), (0x591, 0x5BD), (0x5BF, 0x5BF), (0x5C1, 0x5C2), (0x5C4, 0x5C5),
    (0x5C7, 0x5C9), (0x610, 0x61A), (0x61C, 0x61C), (0x64B, 0x65F), (0x670, 0x670), (0x6D6, 0x6DC),
    (0x6DF, 0x6E4), (0x6E7, 0x6E8), (0x6EA, 0x6ED), (0x711, 0x711), (0x730, 0x74A), (0x7A6, 0x7B0),
    (0x7EB, 0x7F3), (0x7FD, 0x7FD), (0x816, 0x819), (0x81B, 0x823), (0x825, 0x827), (0x829, 0x82D),
    (0x859, 0x85B), (0x897, 0x89F), (0x8CA, 0x8E1), (0x8E3, 0x903), (0x93A, 0x93C), (0x93E, 0x94F),
    (0x951, 0x957), (0x962, 0x963), (0x981, 0x983), (0x9BC, 0x9BC), (0x9BE, 0x9C4), (0x9C7, 0x9C8),
    (0x9CB, 0x9CD), (0x9D7, 0x9D7), (0x9E2, 0x9E3), (0x9FE, 0x9FE), (0xA01, 0xA03), (0xA3C, 0xA3C),
    (0xA3E, 0xA42), (0xA47, 0xA48), (0xA4B, 0xA4D), (0xA51, 0xA51), (0xA70, 0xA71), (0xA75, 0xA75),
    (0xA81, 0xA83), (0xABC, 0xABC), (0xABE, 0xAC5), (0xAC7, 0xAC9), (0xACB, 0xACD), (0xAE2, 0xAE3),
    (0xAFA, 0xAFF), (0xB01, 0xB03), (0xB3C, 0xB3C), (0xB3E, 0xB44), (0xB47, 0xB48), (0xB4B, 0xB4D),
    (0xB53, 0xB57), (0xB62, 0xB63), (0xB82, 0xB82), (0xBBE, 0xBC2), (0xBC6, 0xBC8), (0xBCA, 0xBCD),
    (0xBD7, 0xBD7), (0xC00, 0xC04), (0xC3C, 0xC3C), (0xC3E, 0xC44), (0xC46, 0xC48), (0xC4A, 0xC4D),
    (0xC55, 0xC56), (0xC62, 0xC63), (0xC81, 0xC83), (0xCBC, 0xCBC), (0xCBE, 0xCC4), (0xCC6, 0xCC8),
    (0xCCA, 0xCCD), (0xCD5, 0xCD6), (0xCE2, 0xCE3), (0xCF3, 0xCF3), (0xD00, 0xD03), (0xD3B, 0xD3C),
    (0xD3E, 0xD44), (0xD46, 0xD48), (0xD4A, 0xD4D), (0xD57, 0xD57), (0xD62, 0xD63), (0xD81, 0xD83),
    (0xDCA, 0xDCA), (0xDCF, 0xDD4), (0xDD6, 0xDD6), (0xDD8, 0xDDF), (0xDF2, 0xDF3), (0xE31, 0xE31),
    (0xE34, 0xE3A), (0xE47, 0xE4E), (0xEB1, 0xEB1), (0xEB4, 0xEBC), (0xEC8, 0xECE), (0xF18, 0xF19),
    (0xF35, 0xF35), (0xF37, 0xF37), (0xF39, 0xF39), (0xF3E, 0xF3F), (0xF71, 0xF84), (0xF86, 0xF87),
    (0xF8D, 0xF97), (0xF99, 0xFBC), (0xFC6, 0xFC6), (0x102B, 0x103E), (0x1056, 0x1059), (0x105E, 0x1060),
    (0x1062, 0x1064), (0x1067, 0x106D), (0x1071, 0x1074), (0x1082, 0x108D), (0x108F, 0x108F), (0x109A, 0x109D),
    (0x1160, 0x11FF), (0x135D, 0x135F), (0x1712, 0x1715), (0x1732, 0x1734), (0x1752, 0x1753), (0x1772, 0x1773),
    (0x17B4, 0x17D3), (0x17DD, 0x17DD), (0x180B, 0x180F), (0x1885, 0x1886), (0x18A9, 0x18A9), (0x1920, 0x192B),
    (0x1930, 0x193B), (0x1A17, 0x1A1B), (0x1A55, 0x1A5E), (0x1A60, 0x1A7C), (0x1A7F, 0x1A7F), (0x1AB0, 0x1AF0),
    (0x1B00, 0x1B04), (0x1B34, 0x1B44), (0x1B6B, 0x1B73), (0x1B80, 0x1B82), (0x1BA1, 0x1BAD), (0x1BE6, 0x1BF3),
    (0x1C24, 0x1C37), (0x1CD0, 0x1CD2), (0x1CD4, 0x1CE8), (0x1CED, 0x1CED), (0x1CF4, 0x1CF4), (0x1CF7, 0x1CF9),
    (0x1DC0, 0x1DFF), (0x200B, 0x200F), (0x2028, 0x202E), (0x2060, 0x206F), (0x20D0, 0x20F0), (0x2CEF, 0x2CF1),
    (0x2D7F, 0x2D7F), (0x2DE0, 0x2DFF), (0x302A, 0x302F), (0x3099, 0x309A), (0x3164, 0x3164), (0xA66F, 0xA672),
    (0xA674, 0xA67D), (0xA69E, 0xA69F), (0xA6F0, 0xA6F1), (0xA802, 0xA802), (0xA806, 0xA806), (0xA80B, 0xA80B),
    (0xA823, 0xA827), (0xA82C, 0xA82C), (0xA880, 0xA881), (0xA8B4, 0xA8C5), (0xA8E0, 0xA8F1), (0xA8FF, 0xA8FF),
    (0xA926, 0xA92D), (0xA947, 0xA953), (0xA980, 0xA983), (0xA9B3, 0xA9C0), (0xA9E5, 0xA9E5), (0xAA29, 0xAA36),
    (0xAA43, 0xAA43), (0xAA4C, 0xAA4D), (0xAA7B, 0xAA7D), (0xAAB0, 0xAAB0), (0xAAB2, 0xAAB4), (0xAAB7, 0xAAB8),
    (0xAABE, 0xAABF), (0xAAC1, 0xAAC1), (0xAAEB, 0xAAEF), (0xAAF5, 0xAAF6), (0xABE3, 0xABEA), (0xABEC, 0xABED),
    (0xD7B0, 0xD7FF), (0xFB1E, 0xFB1E), (0xFE00, 0xFE0F), (0xFE20, 0xFE2F), (0xFEFF, 0xFEFF), (0xFFA0, 0xFFA0),
    (0xFFF0, 0xFFFB), (0x101FD, 0x101FD), (0x102E0, 0x102E0), (0x10376, 0x1037A), (0x10A01, 0x10A03), (0x10A05, 0x10A06),
    (0x10A0C, 0x10A0F), (0x10A38, 0x10A3A), (0x10A3F, 0x10A3F), (0x10AE5, 0x10AE6), (0x10D24, 0x10D27), (0x10D69, 0x10D6D),
    (0x10EAB, 0x10EAC), (0x10ECB, 0x10ECF), (0x10EF0, 0x10EFF), (0x10F46, 0x10F50), (0x10F82, 0x10F85), (0x11000, 0x11002),
    (0x11038, 0x11046), (0x11070, 0x11070), (0x11073, 0x11074), (0x1107F, 0x11082), (0x110B0, 0x110BA), (0x110C2, 0x110C2),
    (0x11100, 0x11102), (0x11127, 0x11134), (0x11145, 0x11146), (0x11173, 0x11173), (0x11180, 0x11182), (0x111B3, 0x111C0),
    (0x111C9, 0x111CC), (0x111CE, 0x111CF), (0x1122C, 0x11237), (0x1123E, 0x1123E), (0x11241, 0x11241), (0x112DF, 0x112EA),
    (0x11300, 0x11303), (0x1133B, 0x1133C), (0x1133E, 0x11344), (0x11347, 0x11348), (0x1134B, 0x1134D), (0x11357, 0x11357),
    (0x11362, 0x11363), (0x11366, 0x1136C), (0x11370, 0x11374), (0x113B8, 0x113C0), (0x113C2, 0x113C2), (0x113C5, 0x113C5),
    (0x113C7, 0x113CA), (0x113CC, 0x113D0), (0x113D2, 0x113D2), (0x113E1, 0x113E2), (0x11435, 0x11446), (0x1145E, 0x1145E),
    (0x114B0, 0x114C3), (0x115AF, 0x115B5), (0x115B8, 0x115C0), (0x115DC, 0x115DD), (0x11630, 0x11640), (0x116AB, 0x116B7),
    (0x1171D, 0x1172B), (0x1182C, 0x1183A), (0x11930, 0x11935), (0x11937, 0x11938), (0x1193B, 0x1193E), (0x11940, 0x11940),
    (0x11942, 0x11943), (0x119D1, 0x119D7), (0x119DA, 0x119E0), (0x119E4, 0x119E4), (0x11A01, 0x11A0A), (0x11A33, 0x11A39),
    (0x11A3B, 0x11A3E), (0x11A47, 0x11A47), (0x11A51, 0x11A5B), (0x11A8A, 0x11A99), (0x11B60, 0x11B67), (0x11C2F, 0x11C36),
    (0x11C38, 0x11C3F), (0x11C92, 0x11CA7), (0x11CA9, 0x11CB6), (0x11D31, 0x11D36), (0x11D3A, 0x11D3A), (0x11D3C, 0x11D3D),
    (0x11D3F, 0x11D45), (0x11D47, 0x11D47), (0x11D8A, 0x11D8E), (0x11D90, 0x11D91), (0x11D93, 0x11D97), (0x11DF0, 0x11DF0),
    (0x11EF3, 0x11EF6), (0x11F00, 0x11F01), (0x11F03, 0x11F03), (0x11F34, 0x11F3A), (0x11F3E, 0x11F42), (0x11F5A, 0x11F5A),
    (0x13430, 0x13440), (0x13447, 0x13455), (0x1611E, 0x1612F), (0x16AF0, 0x16AF4), (0x16B30, 0x16B36), (0x16F4F, 0x16F4F),
    (0x16F51, 0x16F87), (0x16F8F, 0x16F92), (0x16FE4, 0x16FE4), (0x16FF0, 0x16FF1), (0x1BC9D, 0x1BC9E), (0x1BCA0, 0x1BCA3),
    (0x1CF00, 0x1CF2D), (0x1CF30, 0x1CF46), (0x1D127, 0x1D128), (0x1D165, 0x1D169), (0x1D16D, 0x1D182), (0x1D185, 0x1D18B),
    (0x1D1AA, 0x1D1AD), (0x1D242, 0x1D244), (0x1D250, 0x1D252), (0x1D25B, 0x1D25C), (0x1D25F, 0x1D25F), (0x1D280, 0x1D281),
    (0x1DA00, 0x1DA36), (0x1DA3B, 0x1DA6C), (0x1DA75, 0x1DA75), (0x1DA84, 0x1DA84), (0x1DA9B, 0x1DA9F), (0x1DAA1, 0x1DAAF),
    (0x1E000, 0x1E006), (0x1E008, 0x1E018), (0x1E01B, 0x1E021), (0x1E023, 0x1E024), (0x1E026, 0x1E02A), (0x1E08F, 0x1E08F),
    (0x1E130, 0x1E136), (0x1E2AE, 0x1E2AE), (0x1E2EC, 0x1E2EF), (0x1E4EC, 0x1E4EF), (0x1E5EE, 0x1E5EF), (0x1E6E3, 0x1E6E3),
    (0x1E6E6, 0x1E6E6), (0x1E6EE, 0x1E6EF), (0x1E6F5, 0x1E6F5), (0x1E8D0, 0x1E8D6), (0x1E944, 0x1E94A), (0xE0000, 0xE0FFF),
];
const WIDE: [(u32, u32); 128] = [
    (0x1100, 0x115F), (0x231A, 0x231B), (0x2329, 0x232A), (0x23E9, 0x23EC), (0x23F0, 0x23F0), (0x23F3, 0x23F3),
    (0x25FD, 0x25FE), (0x2614, 0x2615), (0x2630, 0x2637), (0x2648, 0x2653), (0x267F, 0x267F), (0x268A, 0x268F),
    (0x2693, 0x2693), (0x26A1, 0x26A1), (0x26AA, 0x26AB), (0x26BD, 0x26BE), (0x26C4, 0x26C5), (0x26CE, 0x26CE),
    (0x26D4, 0x26D4), (0x26EA, 0x26EA), (0x26F2, 0x26F3), (0x26F5, 0x26F5), (0x26FA, 0x26FA), (0x26FD, 0x26FD),
    (0x2705, 0x2705), (0x270A, 0x270B), (0x2728, 0x2728), (0x274C, 0x274C), (0x274E, 0x274E), (0x2753, 0x2755),
    (0x2757, 0x2757), (0x2795, 0x2797), (0x27B0, 0x27B0), (0x27BF, 0x27BF), (0x2B1B, 0x2B1C), (0x2B50, 0x2B50),
    (0x2B55, 0x2B55), (0x2E80, 0x2E99), (0x2E9B, 0x2EF3), (0x2F00, 0x2FD5), (0x2FF0, 0x3029), (0x3030, 0x303E),
    (0x3041, 0x3096), (0x309B, 0x30FF), (0x3105, 0x312F), (0x3131, 0x3163), (0x3165, 0x318E), (0x3190, 0x31E5),
    (0x31EF, 0x321E), (0x3220, 0x3247), (0x3250, 0xA48C), (0xA490, 0xA4C6), (0xA960, 0xA97C), (0xAC00, 0xD7A3),
    (0xF900, 0xFAFF), (0xFE10, 0xFE19), (0xFE30, 0xFE52), (0xFE54, 0xFE66), (0xFE68, 0xFE6B), (0xFF01, 0xFF60),
    (0xFFE0, 0xFFE6), (0x16FE0, 0x16FE3), (0x16FF2, 0x16FF6), (0x17000, 0x18CDA), (0x18CFF, 0x18D20), (0x18D80, 0x18DF2),
    (0x18E00, 0x19191), (0x191A0, 0x191D2), (0x1AFF0, 0x1AFF3), (0x1AFF5, 0x1AFFB), (0x1AFFD, 0x1AFFE), (0x1B000, 0x1B128),
    (0x1B132, 0x1B132), (0x1B150, 0x1B152), (0x1B155, 0x1B155), (0x1B164, 0x1B168), (0x1B170, 0x1B2FB), (0x1D300, 0x1D356),
    (0x1D360, 0x1D376), (0x1F004, 0x1F004), (0x1F0CF, 0x1F0CF), (0x1F18E, 0x1F18E), (0x1F191, 0x1F19A), (0x1F1AE, 0x1F1AE),
    (0x1F1E6, 0x1F202), (0x1F210, 0x1F23B), (0x1F240, 0x1F248), (0x1F250, 0x1F251), (0x1F260, 0x1F265), (0x1F300, 0x1F320),
    (0x1F32D, 0x1F335), (0x1F337, 0x1F37C), (0x1F37E, 0x1F393), (0x1F3A0, 0x1F3CA), (0x1F3CF, 0x1F3D3), (0x1F3E0, 0x1F3F0),
    (0x1F3F4, 0x1F3F4), (0x1F3F8, 0x1F43E), (0x1F440, 0x1F440), (0x1F442, 0x1F4FC), (0x1F4FF, 0x1F53D), (0x1F54B, 0x1F54E),
    (0x1F550, 0x1F567), (0x1F57A, 0x1F57A), (0x1F595, 0x1F596), (0x1F5A4, 0x1F5A4), (0x1F5FB, 0x1F64F), (0x1F680, 0x1F6C5),
    (0x1F6CC, 0x1F6CC), (0x1F6D0, 0x1F6D2), (0x1F6D5, 0x1F6D9), (0x1F6DC, 0x1F6DF), (0x1F6EB, 0x1F6EC), (0x1F6F4, 0x1F6FC),
    (0x1F7DA, 0x1F7DA), (0x1F7E0, 0x1F7EB), (0x1F7F0, 0x1F7F0), (0x1F90C, 0x1F93A), (0x1F93C, 0x1F945), (0x1F947, 0x1F9FF),
    (0x1FA70, 0x1FA7C), (0x1FA80, 0x1FAC6), (0x1FAC8, 0x1FAC8), (0x1FACC, 0x1FADD), (0x1FADF, 0x1FAEB), (0x1FAEF, 0x1FAFA),
    (0x20000, 0x2FFFD), (0x30000, 0x3FFFD),
];
// END GENERATED

#[cfg(not(test))]
mod hw {
    use super::Term;
    use crate::port::{inb, outb};

    static mut TERM: Term = Term::new();

    pub fn t() -> &'static mut Term {
        unsafe { &mut *core::ptr::addr_of_mut!(TERM) }
    }

    /// Attribute Controller index 0x10 (mode control) bit 3: 1 = attribute bit 7 blinks, 0 = it selects the bright
    /// background colours 8..15. Returns (before, after). Index 0x30 keeps PAS (bit 5) set so the screen stays on.
    pub fn blink_off() -> (u8, u8) {
        unsafe {
            inb(0x3DA); // reset the index/data flip-flop
            outb(0x3C0, 0x30);
            let old = inb(0x3C1);
            outb(0x3C0, old & !0x08);
            inb(0x3DA);
            outb(0x3C0, 0x30);
            let new = inb(0x3C1);
            inb(0x3DA);
            outb(0x3C0, 0x20); // index write with PAS set (display on); the flip-flop now expects data
            (old, new)
        }
    }

    pub fn flush() {
        let t = t();
        for i in 0..t.w * t.h {
            crate::console::cell64(i, t.cells[i]); // only changed cells are redrawn (VGA or framebuffer)
        }
        crate::console::cursor(if t.cursor_on { Some(t.row * t.w + t.col.min(t.w - 1)) } else { None });
    }
}

/// Take over the screen: bright backgrounds instead of blink, reset the terminal, clear the screen, home the cursor.
#[cfg(not(test))]
pub fn init() {
    let (old, new) = hw::blink_off();
    crate::kprintln!("term: VGA attribute mode control 0x{:02x} -> 0x{:02x} (bit 3 blink {})", old, new, if new & 8 == 0 { "off: bright backgrounds" } else { "STILL ON" });
    hw::t().resize(crate::console::cols(), crate::console::rows()); // the screen's grid (80x25 VGA, larger on the panel)
    hw::flush();
}

/// F11 in a session: the console's new grid, keeping the terminal's modes (see Term::regrid).
#[cfg(not(test))]
pub fn regrid() {
    hw::t().regrid(crate::console::cols(), crate::console::rows());
    hw::flush();
}

/// Replies the terminal owes the host (DSR, DA); the caller sends them on the channel.
#[cfg(not(test))]
pub fn take_reply(out: &mut [u8]) -> usize {
    hw::t().take_reply(out)
}

/// Replies sent so far: DSR 6n, DSR 5n, DA1, DA2, dropped.
#[cfg(not(test))]
pub fn reply_counts() -> [u32; 5] {
    hw::t().counts
}

/// Feed one byte of the remote's output (drawn on the next `flush`).
#[cfg(not(test))]
pub fn feed(b: u8) {
    hw::t().feed(b);
}

/// Copy the grid to VGA memory and move the hardware cursor (once per batch of output, not per byte).
#[cfg(not(test))]
pub fn flush() {
    hw::flush();
}

/// The remote's mouse tracking: (0 / 1000 / 1002 / 1003, SGR encoding).
#[cfg(not(test))]
pub fn mouse() -> (u16, bool) {
    hw::t().mouse()
}

/// DECCKM (CSI ?1h): the keyboard sends ESC O A.. for arrows instead of ESC [ A..
#[cfg(not(test))]
pub fn app_cursor() -> bool {
    hw::t().app_cursor()
}
