//! PS/2 keyboard, polled through the i8042 (data 0x60, status 0x64), scancode set 1.
//!
//! Set 1 because the firmware leaves the i8042's set-2 -> set-1 translation on (SeaBIOS in QEMU does;
//! on the NC110 the internal keyboard sits behind the EC's i8042 emulation, which is the same 0x60/0x64
//! interface; its translation state is unverified until M1-on-hardware). We never reprogram the controller.
//! Output is what an xterm would send: Enter = CR, Backspace = DEL (0x7F), Ctrl+letter = letter & 0x1F,
//! keys as TERM=xterm's terminfo expects: arrows/Home/End as ESC [ x, or ESC O x once the remote set DECCKM (CSI ?1h,
//! ncurses' smkx); PgUp/PgDn/Insert/Delete ESC [ n ~; F1-F4 ESC O P..S; F5-F12 ESC [ 15/17/18/19/20/21/23/24 ~. US layout only. Ctrl-Alt-Del is the local escape (poll returns true).
//! `init` reads the controller command byte: with translation off (raw set 2) the keyboard is left disabled
//! rather than decoded wrongly.
use crate::port::{inb, outb};

const DATA: u16 = 0x60;
const STATUS: u16 = 0x64;

static mut SHIFT_L: bool = false;
static mut SHIFT_R: bool = false;
static mut CTRL_L: bool = false;
static mut CTRL_R: bool = false;
static mut CAPS: bool = false;
static mut ALT_L: bool = false;
static mut ALT_R: bool = false;
static mut E0: bool = false;
static mut ENABLED: bool = false;
static mut DROPPED: u32 = 0;

// Set 1 make codes 0x00..0x3A, US layout: (unshifted, shifted); 0 = not a character key.
const MAP: [(u8, u8); 0x3A] = {
    let mut m = [(0u8, 0u8); 0x3A];
    m[0x01] = (0x1B, 0x1B);
    let row1 = b"1234567890-=";
    let row1s = b"!@#$%^&*()_+";
    let mut i = 0;
    while i < 12 {
        m[0x02 + i] = (row1[i], row1s[i]);
        i += 1;
    }
    m[0x0E] = (0x7F, 0x7F);
    m[0x0F] = (b'\t', b'\t');
    let row2 = b"qwertyuiop[]";
    let row2s = b"QWERTYUIOP{}";
    i = 0;
    while i < 12 {
        m[0x10 + i] = (row2[i], row2s[i]);
        i += 1;
    }
    m[0x1C] = (b'\r', b'\r');
    let row3 = b"asdfghjkl;'`";
    let row3s = b"ASDFGHJKL:\"~";
    i = 0;
    while i < 12 {
        m[0x1E + i] = (row3[i], row3s[i]);
        i += 1;
    }
    m[0x2B] = (b'\\', b'|');
    let row4 = b"zxcvbnm,./";
    let row4s = b"ZXCVBNM<>?";
    i = 0;
    while i < 10 {
        m[0x2C + i] = (row4[i], row4s[i]);
        i += 1;
    }
    m[0x37] = (b'*', b'*');
    m[0x39] = (b' ', b' ');
    m
};

fn drain() -> usize {
    let mut n = 0;
    unsafe {
        while inb(STATUS) & 1 != 0 && n < 64 {
            let _ = inb(DATA);
            n += 1;
        }
    }
    n
}

/// Drain firmware leftovers, then read the i8042 command byte (cmd 0x20) and require translation (bit 6):
/// that is what makes the keyboard's set 2 arrive here as set 1. Returns the command byte if it was readable.
/// Keyboard delivery is paused (0xAD) around the read so a keystroke cannot be mistaken for the reply.
fn cmd(c: u8) -> bool {
    unsafe {
        for _ in 0..1_000_000 {
            if inb(STATUS) & 2 == 0 {
                crate::port::outb(STATUS, c);
                return true;
            }
        }
    }
    false
}

pub fn init() -> Option<u8> {
    let paused = cmd(0xAD); // disable keyboard interface
    drain();
    let cfg = unsafe {
        if paused && cmd(0x20) {
            let mut v = None;
            for _ in 0..1_000_000 {
                let st = inb(STATUS);
                if st & 1 != 0 {
                    let b = inb(DATA);
                    if st & 0x20 == 0 {
                        v = Some(b);
                        break;
                    }
                }
            }
            v
        } else {
            None
        }
    };
    unsafe {
        ENABLED = matches!(cfg, Some(c) if c & 0x40 != 0);
        crate::kprintln!(
            "kbd: i8042 command byte {:?} -> translation {}",
            cfg,
            if ENABLED { "on, decoding scancode set 1" } else { "OFF or unreadable: keyboard disabled (set 2 not supported)" }
        );
    }
    drain();
    cmd(0xAE); // re-enable keyboard interface (even if the read failed: never leave it off)
    if let Some(c) = cfg {
        mouse_init(c);
    }
    cfg
}

static mut MOUSE: bool = false; // AUX device reporting
static mut WHEEL: bool = false; // IntelliMouse 4-byte packets
static mut PKT: [u8; 4] = [0; 4];
static mut NPKT: usize = 0;
/// Decoded packets, oldest first (dx, dy, wheel, buttons): one per packet so no button transition is merged away.
static mut MQ: [(i32, i32, i32, u8); 16] = [(0, 0, 0, 0); 16];
static mut MQN: usize = 0;

/// Read one byte from the controller (the AUX bit must match `aux`), bounded.
fn read_byte(aux: bool) -> Option<u8> {
    unsafe {
        for _ in 0..200_000 {
            let st = inb(STATUS);
            if st & 1 != 0 {
                let b = inb(DATA);
                if (st & 0x20 != 0) == aux {
                    return Some(b);
                }
            }
        }
    }
    None
}

/// Send a byte to the AUX device (touchpad in PS/2 mouse mode) and wait for its ACK (0xFA).
fn aux(b: u8) -> bool {
    if !cmd(0xD4) {
        return false;
    }
    unsafe {
        for _ in 0..1_000_000 {
            if inb(STATUS) & 2 == 0 {
                outb(DATA, b);
                return read_byte(true) == Some(0xFA);
            }
        }
    }
    false
}

/// Once per boot: AUX port on with its clock enabled and interrupts off (we poll), defaults, IntelliMouse wheel probe
/// (sample rate 200, 100, 80 then ID 3 = 4-byte packets), reporting on. Every step is bounded; failure leaves no mouse.
fn mouse_init(cfg: u8) {
    if unsafe { MOUSE } {
        return;
    }
    cmd(0xA8); // enable AUX interface
    if cmd(0x60) {
        unsafe {
            for _ in 0..1_000_000 {
                if inb(STATUS) & 2 == 0 {
                    // keyboard + AUX clocks on (cfg was read while init() had the keyboard paused: bit 4 set), AUX IRQ off
                    outb(DATA, cfg & !0x10 & !0x20 & !0x02);
                    break;
                }
            }
        }
    }
    drain();
    // Full reset first: a pad Linux's elantech driver switched to absolute mode stays there across a warm reset, and
    // 0xF6 does not undo it (measured: 6-byte Elantech packets, proof/nc110-touchpad-raw-elantech.log). 0xFF does: ACK,
    // then the self-test result 0xAA and the ID 0x00 within ~500 ms.
    let mut bat = None;
    if aux(0xFF) {
        let t = crate::time::now_ms();
        while bat.is_none() && crate::time::now_ms() - t < 1500 {
            bat = read_byte(true);
        }
        let _ = read_byte(true); // device ID after the self-test
    }
    let ok = aux(0xF6) && aux(0xF3) && aux(200) && aux(0xF3) && aux(100) && aux(0xF3) && aux(80) && aux(0xF2);
    let id = if ok { read_byte(true) } else { None };
    let on = ok && aux(0xF4);
    unsafe {
        WHEEL = id == Some(3);
        MOUSE = on;
    }
    crate::kprintln!("kbd: touchpad/mouse {} (reset self-test {:x?}, id {:?}{})", if on { "on" } else { "NOT answering" }, bat, id,
        if id == Some(3) { ", wheel" } else { "" });
}

static mut LAST_AUX: i64 = 0;
static mut RAW: [u8; 48] = [0; 48];
static mut NRAW: usize = 0;

/// The last raw AUX bytes, oldest first (bmctl `mouse`, for debugging the pad).
pub fn raw_aux(out: &mut [u8; 48]) -> usize {
    unsafe {
        let r = &*core::ptr::addr_of!(RAW);
        let n = NRAW.min(48);
        for i in 0..n {
            out[i] = r[(NRAW - n + i) % 48];
        }
        n
    }
}

fn aux_byte(b: u8) {
    unsafe {
        (*core::ptr::addr_of_mut!(RAW))[NRAW % 48] = b;
        NRAW += 1;
        // Resync like Linux psmouse: the bytes of one packet arrive back to back, so a gap means a new packet.
        let now = crate::time::now_ms();
        if NPKT > 0 && now - LAST_AUX > 30 {
            NPKT = 0;
        }
        LAST_AUX = now;
        if NPKT == 0 && (b & 0x08 == 0 || b & 0xC0 != 0) {
            return; // not a plausible byte 0: bit 3 always set, X/Y overflow (bits 6/7) treated as noise
        }
        PKT[NPKT] = b;
        NPKT += 1;
        if NPKT < if WHEEL { 4 } else { 3 } {
            return;
        }
        NPKT = 0;
        let b0 = PKT[0];
        let (dx, dy) = if b0 & 0xC0 == 0 {
            (PKT[1] as i32 - ((b0 as i32) << 4 & 0x100), PKT[2] as i32 - ((b0 as i32) << 3 & 0x100))
        } else {
            (0, 0) // overflow: drop the motion, keep the buttons
        };
        let w = if WHEEL { ((PKT[3] << 4) as i8 >> 4) as i32 } else { 0 };
        let q = &mut *core::ptr::addr_of_mut!(MQ);
        if MQN > 0 && q[MQN - 1].3 == b0 & 7 && w == 0 && q[MQN - 1].2 == 0 {
            q[MQN - 1].0 += dx; // same buttons, plain motion: coalesce
            q[MQN - 1].1 += dy;
        } else if MQN < q.len() {
            q[MQN] = (dx, dy, w, b0 & 7);
            MQN += 1;
        }
    }
}

/// The oldest queued mouse event, merged with the moves after it that keep the same buttons: (dx, dy up-positive, wheel,
/// buttons bit0 L / bit1 R / bit2 M).
pub fn take_mouse() -> Option<(i32, i32, i32, u8)> {
    unsafe {
        if MQN == 0 {
            return None;
        }
        let q = &mut *core::ptr::addr_of_mut!(MQ);
        // Coalesce: following packets with the same buttons merge into one move (a busy SSH pass used to take one
        // packet each, so the pointer lagged behind the finger). Button changes stay separate events.
        let mut r = q[0];
        let mut k = 1;
        while k < MQN && q[k].3 == r.3 {
            (r.0, r.1, r.2) = (r.0 + q[k].0, r.1 + q[k].1, r.2 + q[k].2);
            k += 1;
        }
        q.copy_within(k..MQN, 0);
        MQN -= k;
        Some(r)
    }
}

/// Translate one scancode; appends 0..8 bytes to `out` and returns how many.
fn translate(sc: u8, out: &mut [u8; 8]) -> usize {
    unsafe {
        if sc == 0xE0 {
            E0 = true;
            return 0;
        }
        let ext = E0;
        E0 = false;
        let brk = sc & 0x80 != 0;
        let code = sc & 0x7F;
        match (ext, code) {
            (false, 0x2A) => SHIFT_L = !brk,
            (false, 0x36) => SHIFT_R = !brk,
            (false, 0x1D) => CTRL_L = !brk,
            (true, 0x1D) => CTRL_R = !brk,
            (false, 0x38) => ALT_L = !brk,
            (true, 0x38) => ALT_R = !brk,
            (false, 0x3A) if !brk => CAPS = !CAPS,
            _ => {}
        }
        if brk {
            return 0;
        }
        let app = crate::term::app_cursor();
        let seq: &[u8] = match (ext, code) {
            (true, 0x48) => if app { b"\x1bOA" } else { b"\x1b[A" },
            (true, 0x50) => if app { b"\x1bOB" } else { b"\x1b[B" },
            (true, 0x4D) => if app { b"\x1bOC" } else { b"\x1b[C" },
            (true, 0x4B) => if app { b"\x1bOD" } else { b"\x1b[D" },
            (true, 0x47) => if app { b"\x1bOH" } else { b"\x1b[H" },
            (true, 0x4F) => if app { b"\x1bOF" } else { b"\x1b[F" },
            (true, 0x49) => b"\x1b[5~",
            (true, 0x51) => b"\x1b[6~",
            (true, 0x52) => b"\x1b[2~",
            (true, 0x53) => b"\x1b[3~",
            (true, 0x1C) => b"\r", // keypad Enter
            (true, 0x35) => b"/",  // keypad /
            (true, _) => b"",
            (false, 0x3B) => b"\x1bOP",
            (false, 0x3C) => b"\x1bOQ",
            (false, 0x3D) => b"\x1bOR",
            (false, 0x3E) => b"\x1bOS",
            (false, 0x3F) => b"\x1b[15~",
            (false, 0x40) => b"\x1b[17~",
            (false, 0x41) => b"\x1b[18~",
            (false, 0x42) => b"\x1b[19~",
            (false, 0x43) => b"\x1b[20~",
            (false, 0x44) => b"\x1b[21~",
            (false, 0x57) => b"\x1b[23~",
            (false, 0x58) => b"\x1b[24~",
            _ => b"",
        };
        if ext || !seq.is_empty() {
            out[..seq.len()].copy_from_slice(seq);
            return seq.len();
        }
        if (code as usize) >= MAP.len() {
            return 0;
        }
        let (lo, hi) = MAP[code as usize];
        if lo == 0 {
            return 0;
        }
        let shift = SHIFT_L || SHIFT_R;
        let mut c = if shift { hi } else { lo };
        if CAPS && lo.is_ascii_lowercase() {
            c = if shift { lo } else { hi };
        }
        if CTRL_L || CTRL_R {
            c = match c {
                b'a'..=b'z' | b'A'..=b'Z' => c & 0x1F,
                b'@' | b' ' | b'2' => 0x00,
                b'[' => 0x1B,
                b'\\' => 0x1C,
                b']' => 0x1D,
                b'_' | b'-' => 0x1F,
                _ => c,
            };
        }
        out[0] = c;
        1
    }
}

/// Poll the controller and append the bytes typed so far to `buf[*len..]`. Mouse (AUX) bytes are dropped.
/// Always drains the controller so modifier releases are never missed; if `buf` is full (the SSH channel is not
/// taking input), the key's bytes are dropped and counted rather than left to overflow the controller.
/// Returns true on Ctrl-Alt-Del.
static mut INJ: [u8; 32] = [0; 32];
static mut NINJ: usize = 0;

/// bmctl `key <text>`: bytes delivered by the next `poll` as if typed (an authenticated agent drives the home screen
/// or the terminal; it can already do far more over bmctl).
pub fn inject(b: &[u8]) -> usize {
    unsafe {
        let q = &mut *core::ptr::addr_of_mut!(INJ);
        let n = b.len().min(q.len() - NINJ);
        q[NINJ..NINJ + n].copy_from_slice(&b[..n]);
        NINJ += n;
        n
    }
}

pub fn poll(buf: &mut [u8], len: &mut usize) -> bool {
    unsafe {
        if NINJ > 0 {
            let q = &mut *core::ptr::addr_of_mut!(INJ);
            let n = NINJ.min(buf.len() - *len);
            buf[*len..*len + n].copy_from_slice(&q[..n]);
            *len += n;
            q.copy_within(n..NINJ, 0);
            NINJ -= n;
        }
        if !ENABLED {
            drain();
            return false;
        }
        for _ in 0..64 {
            let st = inb(STATUS);
            if st & 1 == 0 {
                break;
            }
            let sc = inb(DATA);
            if st & 0x20 != 0 {
                aux_byte(sc); // touchpad / mouse packet byte
                continue;
            }
            if E0 && sc == 0x53 && (CTRL_L || CTRL_R) && (ALT_L || ALT_R) {
                return true;
            }
            if !E0 && sc == 0x58 {
                PTT = true; // F12 make: push-to-talk toggle, never reaches the shell
                continue;
            }
            if !E0 && sc == 0x57 {
                SIZE = true; // F11 make: next text size (SSH session), never reaches the shell
                continue;
            }
            let mut tmp = [0u8; 8];
            let n = translate(sc, &mut tmp);
            if buf.len() - *len >= n {
                buf[*len..*len + n].copy_from_slice(&tmp[..n]);
                *len += n;
            } else {
                DROPPED = DROPPED.wrapping_add(1);
            }
        }
        false
    }
}

static mut PTT: bool = false;
static mut SIZE: bool = false;

/// True once per F11 press since the last call (next text size).
pub fn take_size() -> bool {
    unsafe { core::mem::replace(&mut *core::ptr::addr_of_mut!(SIZE), false) }
}

/// True once per F12 press since the last call (push-to-talk toggle).
pub fn take_ptt() -> bool {
    unsafe { core::mem::replace(&mut *core::ptr::addr_of_mut!(PTT), false) }
}

/// Keys dropped because the channel was not taking input.
pub fn dropped() -> u32 {
    unsafe { DROPPED }
}
