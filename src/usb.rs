//! USB, step 1: identify what is plugged in (bmctl `usb`), for the fingerprint reader. Polled, no interrupts.
//! NM10: four UHCI controllers (00:1d.0-3) and one EHCI (00:1d.7). Everything is routed to the UHCI companions: EHCI is
//! taken from the BIOS (USBLEGSUP handoff), halted, and CONFIGFLAG cleared, so a USB 2.0 device simply runs at full
//! speed. Each UHCI controller is taken from BIOS legacy mode (LEGSUP), reset, given a frame list whose every entry
//! points at one queue head, and each connected port is reset and enumerated: device descriptor (VID:PID, class),
//! address, and the first configuration's interface classes. All waits are bounded and pet the watchdog.
use crate::port::{inl, inw, outl, outw};
use crate::{pci, wdt};
use core::fmt::Write;
use core::ptr::{read_volatile, write_volatile};

#[repr(C, align(4096))]
struct FrameList([u32; 1024]);
#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct Td([u32; 4]);
#[repr(C, align(16))]
struct Qh([u32; 4]);

static mut FL: FrameList = FrameList([1; 1024]);
static mut QH: Qh = Qh([1, 1, 0, 0]);
static mut TDS: [Td; 16] = [Td([1, 0, 0, 0]); 16];
static mut BUF: [u8; 512] = [0; 512];
static mut SETUP: [u8; 8] = [0; 8];

/// What `scan` found, kept for `usb ctl/in/out`: controller I/O base, address, low speed, ep0 max packet, and the
/// non-control endpoints (address with direction bit, max packet, next data toggle).
#[derive(Clone, Copy)]
pub struct Dev {
    io: u16,
    addr: u8,
    ls: bool,
    mps0: u16,
    eps: [(u8, u16, u8); 8],
}
static mut DEVS: [Option<Dev>; 8] = [None; 8];

fn addr<T>(p: *const T) -> u32 {
    p as u64 as u32
}

/// EHCI: OS ownership, halt, CONFIGFLAG = 0 (all ports to the companion UHCI controllers).
fn ehci_release(w: &mut dyn Write) {
    let mut found = None;
    pci::walk(&mut |b, _| {
        if pci::read32(b, 8) >> 8 == 0x0C_0320 {
            found = Some(b);
            return true;
        }
        false
    });
    let Some(b) = found else { return };
    let bar = pci::read32(b, 0x10) as u64 & !0xFF;
    if bar == 0 || !crate::nic::map_uc_4k(bar) {
        let _ = write!(w, "ehci: no usable BAR\n");
        return;
    }
    pci::write32(b, 4, pci::read32(b, 4) & 0xFFFF | 0x6);
    let rd = |o: u64| unsafe { read_volatile((bar + o) as *const u32) };
    let wr = |o: u64, v: u32| unsafe { write_volatile((bar + o) as *mut u32, v) };
    let caplen = (rd(0) & 0xFF) as u64;
    let eecp = ((rd(8) >> 8) & 0xFF) as u8;
    if eecp >= 0x40 {
        let leg = pci::read32(b, eecp);
        if leg & 0xFF == 1 {
            pci::write32(b, eecp, leg | (1 << 24)); // HC OS Owned
            for _ in 0..100 {
                if pci::read32(b, eecp) & (1 << 16) == 0 {
                    break;
                }
                wdt::pet();
                wdt::delay_ms(10);
            }
            pci::write32(b, eecp + 4, 0); // USBLEGCTLSTS: no SMIs
        }
    }
    let op = caplen;
    wr(op, rd(op) & !1); // USBCMD RS = 0
    for _ in 0..100 {
        if rd(op + 4) & (1 << 12) != 0 {
            break; // HCHalted
        }
        wdt::delay_ms(1);
    }
    wr(op + 0x40, 0); // CONFIGFLAG = 0: every port to a companion controller
    for n in 0..(rd(4) & 0xF) as u64 {
        let ps = rd(op + 0x44 + 4 * n);
        wr(op + 0x44 + 4 * n, (ps & !0x2A) | (1 << 13)); // PORT_OWNER = companion (change bits RW1C kept clear)
    }
    let _ = write!(w, "ehci {:02x}:{:02x}.{} halted, ports routed to UHCI (legsup {:#x})\n", b.bus, b.dev, b.func,
        if eecp >= 0x40 { pci::read32(b, eecp) } else { 0 });
    wdt::delay_ms(20);
}

struct Uhci(u16);

impl Uhci {
    fn r16(&self, o: u16) -> u16 {
        unsafe { inw(self.0 + o) }
    }
    fn w16(&self, o: u16, v: u16) {
        unsafe { outw(self.0 + o, v) }
    }

    /// One control transfer on device `dev` (endpoint 0, max packet `mps`): SETUP, data IN stages, status OUT.
    /// Returns the bytes received into BUF.
    fn control(&self, dev: u8, mps: u16, ls: bool, setup: [u8; 8], len: u16) -> Result<usize, &'static str> {
        unsafe {
            let tds = &mut *core::ptr::addr_of_mut!(TDS);
            let buf = core::ptr::addr_of_mut!(BUF) as *mut u8;
            core::ptr::copy_nonoverlapping(setup.as_ptr(), core::ptr::addr_of_mut!(SETUP) as *mut u8, 8);
            let ctl = |ioc: bool| (3 << 27) | (if ls { 1 << 26 } else { 0 }) | (if ioc { 1 << 24 } else { 0 }) | (1 << 23);
            let tok = |pid: u32, toggle: u32, n: u16| {
                let ml = if n == 0 { 0x7FF } else { n as u32 - 1 };
                (ml << 21) | (toggle << 19) | ((dev as u32) << 8) | pid
            };
            let mut n = 0usize;
            tds[n] = Td([0, ctl(false), tok(0x2D, 0, 8), addr(core::ptr::addr_of!(SETUP))]);
            n += 1;
            let (mut off, mut toggle) = (0u16, 1u32);
            while off < len && n < tds.len() - 1 {
                let chunk = mps.min(len - off);
                tds[n] = Td([0, ctl(false), tok(0x69, toggle, chunk), addr(buf.add(off as usize))]);
                toggle ^= 1;
                off += chunk;
                n += 1;
            }
            let status_pid = if len > 0 { 0xE1 } else { 0x69 }; // status stage: opposite direction, DATA1
            tds[n] = Td([1, ctl(true), tok(status_pid, 1, 0), 0]);
            for i in 0..n {
                tds[i].0[0] = addr(&tds[i + 1]) | 0x4; // depth first
            }
            let qh = &mut *core::ptr::addr_of_mut!(QH);
            write_volatile(&mut qh.0[1], addr(&tds[0]));
            // wait for the last TD to go inactive, or any TD to stall/error
            for _ in 0..500 {
                wdt::delay_ms(1);
                let last = read_volatile(&tds[n].0[1]);
                if last & (1 << 23) == 0 {
                    break;
                }
                for td in tds[..=n].iter() {
                    let st = read_volatile(&td.0[1]);
                    if st & (1 << 22) != 0 {
                        write_volatile(&mut qh.0[1], 1);
                        return Err("stalled");
                    }
                }
            }
            write_volatile(&mut qh.0[1], 1);
            if read_volatile(&tds[n].0[1]) & (1 << 23) != 0 {
                return Err("timeout");
            }
            // received bytes = sum of actual lengths of the IN data TDs
            let mut got = 0usize;
            for td in tds[1..n].iter() {
                let a = read_volatile(&td.0[1]) & 0x7FF;
                got += if a == 0x7FF { 0 } else { a as usize + 1 };
            }
            Ok(got.min(len as usize))
        }
    }
}

fn buf() -> &'static [u8] {
    unsafe { &*core::ptr::addr_of!(BUF) }
}

/// Enumerate everything plugged into the UHCI controllers (EHCI ports routed to them first) and describe it in `w`.
pub fn scan(w: &mut dyn Write) {
    unsafe { *core::ptr::addr_of_mut!(DEVS) = [None; 8] };
    ehci_release(w);
    let mut ctrls: heapless::Vec<pci::Bdf, 8> = heapless::Vec::new();
    pci::walk(&mut |b, _| {
        if pci::read32(b, 8) >> 8 == 0x0C_0300 {
            let _ = ctrls.push(b);
        }
        false
    });
    let mut next_addr = 1u8;
    for b in ctrls {
        let io = (pci::read32(b, 0x20) & 0xFFE0) as u16;
        if io == 0 {
            continue;
        }
        pci::write32(b, 4, pci::read32(b, 4) & 0xFFFF | 0x5); // I/O + bus master
        pci::write32(b, 0xC0, 0x8F00); // LEGSUP: no SMIs / legacy emulation, clear status (Linux uhci_reset_hc)
        let h = Uhci(io);
        h.w16(0, 0x0004); // GRESET
        wdt::delay_ms(50);
        h.w16(0, 0);
        h.w16(0, 0x0002); // HCRESET
        for _ in 0..100 {
            if h.r16(0) & 2 == 0 {
                break;
            }
            wdt::delay_ms(1);
        }
        unsafe {
            let fl = &mut *core::ptr::addr_of_mut!(FL);
            let q = addr(core::ptr::addr_of!(QH)) | 0x2; // QH pointer
            for e in fl.0.iter_mut() {
                *e = q;
            }
            let qh = &mut *core::ptr::addr_of_mut!(QH);
            qh.0 = [1, 1, 0, 0];
            outl(io + 8, addr(core::ptr::addr_of!(FL)));
            let _ = inl(io + 8);
        }
        h.w16(4, 0); // no interrupts
        h.w16(6, 0); // frame 0
        h.w16(2, 0xFFFF); // clear status
        h.w16(0, 0x0081); // RS + MAXP (64-byte packets)
        wdt::delay_ms(100); // ports re-detect their devices after the global reset
        let _ = write!(w, "uhci {:02x}:{:02x}.{} io {:#x}\n", b.bus, b.dev, b.func, io);
        for p in 0..2u16 {
            let reg = 0x10 + 2 * p;
            let sc = h.r16(reg);
            if sc & 1 == 0 {
                continue; // nothing connected
            }
            h.w16(reg, sc | (1 << 9)); // port reset
            wdt::delay_ms(50);
            h.w16(reg, h.r16(reg) & !(1 << 9));
            wdt::delay_ms(10);
            for _ in 0..10 {
                h.w16(reg, (h.r16(reg) & !0x0A) | 0x0E); // enable, clear change bits
                wdt::delay_ms(10);
                if h.r16(reg) & 4 != 0 {
                    break;
                }
            }
            let sc = h.r16(reg);
            let ls = sc & (1 << 8) != 0;
            wdt::pet();
            let _ = write!(w, "  port {}: {}-speed, portsc {:#x}: ", p + 1, if ls { "low" } else { "full" }, sc);
            // GET_DESCRIPTOR(device) 8 bytes at address 0 -> ep0 max packet
            let dd8 = h.control(0, 8, ls, [0x80, 6, 0, 1, 0, 0, 8, 0], 8);
            let mps = match dd8 {
                Ok(n) if n >= 8 => buf()[7].max(8) as u16,
                Ok(_) | Err(_) => {
                    let _ = write!(w, "no device descriptor ({:?})\n", dd8.err());
                    continue;
                }
            };
            let a = next_addr;
            next_addr += 1;
            if let Err(e) = h.control(0, mps, ls, [0x00, 5, a, 0, 0, 0, 0, 0], 0) {
                let _ = write!(w, "SET_ADDRESS failed ({})\n", e);
                continue;
            }
            wdt::delay_ms(5);
            match h.control(a, mps, ls, [0x80, 6, 0, 1, 0, 0, 18, 0], 18) {
                Ok(n) if n >= 18 => {
                    let d = buf();
                    let (vid, pid) = (u16::from_le_bytes([d[8], d[9]]), u16::from_le_bytes([d[10], d[11]]));
                    let _ = write!(w, "{:04x}:{:04x} usb {:x}.{:02x} class {:02x}/{:02x}/{:02x} mps {}", vid, pid, d[3], d[2],
                        d[4], d[5], d[6], mps);
                }
                other => {
                    let _ = write!(w, "full descriptor failed ({:?})\n", other);
                    continue;
                }
            }
            let mut dev = Dev { io, addr: a, ls, mps0: mps, eps: [(0, 0, 0); 8] };
            let mut ne = 0;
            // configuration descriptor: interface classes
            if let Ok(n) = h.control(a, mps, ls, [0x80, 6, 0, 2, 0, 0, 255, 0], 255) {
                let d = &buf()[..n];
                let mut i = 0;
                while i + 1 < d.len() && d[i] >= 2 {
                    if d[i + 1] == 4 && i + 8 < d.len() {
                        let _ = write!(w, " | if{} class {:02x}/{:02x}/{:02x} eps {}", d[i + 2], d[i + 5], d[i + 6], d[i + 7], d[i + 4]);
                    }
                    if d[i + 1] == 5 && i + 6 < d.len() && ne < 8 {
                        dev.eps[ne] = (d[i + 2], u16::from_le_bytes([d[i + 4], d[i + 5]]), 0);
                        ne += 1;
                        let _ = write!(w, " ep {:02x} {} mps {}", d[i + 2], ["ctl", "iso", "bulk", "int"][(d[i + 3] & 3) as usize],
                            u16::from_le_bytes([d[i + 4], d[i + 5]]));
                    }
                    i += d[i] as usize;
                }
            }
            // SET_CONFIGURATION(1) so the endpoints exist
            let _ = h.control(a, mps, ls, [0x00, 9, 1, 0, 0, 0, 0, 0], 0);
            let devs = unsafe { &mut *core::ptr::addr_of_mut!(DEVS) };
            if let Some(slot) = devs.iter().position(|d| d.is_none()) {
                devs[slot] = Some(dev);
                let _ = write!(w, " -> dev {}", slot);
            }
            let _ = write!(w, "\n");
        }
        h.w16(0, 0); // stop the schedule again (all controllers share one frame list: one runs at a time)
    }
}

impl Uhci {
    /// Make this controller the one running the shared schedule.
    fn start(&self) {
        unsafe {
            outl(self.0 + 8, addr(core::ptr::addr_of!(FL)));
        }
        self.w16(0, 0x0081);
    }

    /// Bulk/interrupt transfer on endpoint `ep` (bit 7 = IN) of `d`: packets of the endpoint's max packet, data
    /// toggle carried in the device table. IN returns the bytes received into BUF (stops at a short packet).
    fn data(&self, d: &mut Dev, ep: u8, out: &[u8], len: usize) -> Result<usize, &'static str> {
        let e = d.eps.iter().position(|x| x.0 == ep && x.1 != 0).ok_or("no such endpoint")?;
        let (mps, mut toggle) = (d.eps[e].1.max(1), d.eps[e].2 as u32);
        let inp = ep & 0x80 != 0;
        let total = if inp { len.min(512) } else { out.len().min(512) };
        unsafe {
            let tds = &mut *core::ptr::addr_of_mut!(TDS);
            let buf = core::ptr::addr_of_mut!(BUF) as *mut u8;
            if !inp {
                core::ptr::copy_nonoverlapping(out.as_ptr(), buf, total);
            }
            let ctl = (3 << 27) | (if d.ls { 1 << 26 } else { 0 }) | (1 << 23) | (if inp { 1 << 29 } else { 0 }); // SPD on IN
            let mut n = 0;
            let mut off = 0usize;
            while (off < total || n == 0) && n < tds.len() {
                let chunk = (mps as usize).min(total - off);
                let ml = if chunk == 0 { 0x7FF } else { chunk as u32 - 1 };
                let pid = if inp { 0x69 } else { 0xE1 };
                tds[n] = Td([1, ctl, (ml << 21) | (toggle << 19) | (((ep & 0xF) as u32) << 15) | ((d.addr as u32) << 8) | pid,
                    addr(buf.add(off))]);
                toggle ^= 1;
                off += chunk;
                n += 1;
            }
            for i in 0..n.saturating_sub(1) {
                tds[i].0[0] = addr(&tds[i + 1]) | 0x4;
            }
            self.start();
            let qh = &mut *core::ptr::addr_of_mut!(QH);
            write_volatile(&mut qh.0[1], addr(&tds[0]));
            let mut res = Err("timeout");
            for _ in 0..1000 {
                wdt::delay_ms(1);
                let mut active = false;
                let mut got = 0usize;
                let mut done_toggles = 0u32;
                for td in tds[..n].iter() {
                    let st = read_volatile(&td.0[1]);
                    if st & (1 << 22) != 0 {
                        res = Err("stalled");
                        active = false;
                        break;
                    }
                    if st & (1 << 23) != 0 {
                        active = true;
                        break;
                    }
                    let a = st & 0x7FF;
                    let a = if a == 0x7FF { 0 } else { a as usize + 1 };
                    got += a;
                    done_toggles += 1;
                    if inp && a < mps as usize {
                        break; // short packet ends the transfer
                    }
                }
                if !active {
                    if res.is_ok() || res == Err("timeout") {
                        d.eps[e].2 = ((d.eps[e].2 as u32 + done_toggles) & 1) as u8;
                        res = Ok(got);
                    }
                    break;
                }
            }
            write_volatile(&mut qh.0[1], 1);
            self.w16(0, 0);
            res
        }
    }
}

/// bmctl `usb ctl <dev> <setup: 16 hex> [len]`, `usb in <dev> <ep hex> <len>`, `usb out <dev> <ep hex> <data hex>`.
pub fn cmd<'a>(w: &mut dyn Write, it: &mut impl Iterator<Item = &'a str>, sub: &str) {
    let devs = unsafe { &mut *core::ptr::addr_of_mut!(DEVS) };
    let Some(d) = it.next().and_then(|x| x.parse::<usize>().ok()).and_then(|i| devs.get_mut(i)).and_then(|x| x.as_mut()) else {
        let _ = write!(w, "err no such device (run usb first)");
        return;
    };
    let hex = |s: &str, out: &mut [u8]| -> Option<usize> {
        if s.len() % 2 != 0 || s.len() / 2 > out.len() || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        for i in 0..s.len() / 2 {
            out[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
        }
        Some(s.len() / 2)
    };
    let h = Uhci(d.io);
    let r = match sub {
        "ctl" => {
            let mut st = [0u8; 8];
            match it.next().and_then(|x| hex(x, &mut st)) {
                Some(8) => {
                    let len = it.next().and_then(|x| x.parse::<u16>().ok()).unwrap_or(u16::from_le_bytes([st[6], st[7]]));
                    h.start();
                    let r = h.control(d.addr, d.mps0, d.ls, st, len.min(512));
                    h.w16(0, 0);
                    r
                }
                _ => Err("setup must be 16 hex digits"),
            }
        }
        "in" => match it.next().and_then(|x| u8::from_str_radix(x, 16).ok()) {
            Some(ep) => h.data(d, ep | 0x80, &[], it.next().and_then(|x| x.parse().ok()).unwrap_or(64)),
            None => Err("ep"),
        },
        "out" => {
            let ep = it.next().and_then(|x| u8::from_str_radix(x, 16).ok());
            let mut b = [0u8; 512];
            match (ep, it.next().and_then(|x| hex(x, &mut b))) {
                (Some(ep), Some(n)) => h.data(d, ep & 0x7F, &b[..n], 0).map(|_| 0),
                _ => Err("ep / data hex"),
            }
        }
        _ => Err("usb [ctl|in|out]"),
    };
    match r {
        Ok(n) => {
            let _ = write!(w, "ok {}:", n);
            for b in &buf()[..n.min(512)] {
                let _ = write!(w, " {:02x}", b);
            }
        }
        Err(e) => {
            let _ = write!(w, "err {}", e);
        }
    }
}
