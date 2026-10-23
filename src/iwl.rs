//! The Intel 130's DVM bring-up past ALIVE, polled (docs/iwl-dvm-bringup.md, from Linux 6.18 iwlwifi):
//! INIT image -> calibration results -> device stop -> runtime image -> configuration -> passive scan.
//! bmctl `wifi up` runs the whole sequence; `wifi scan` starts a scan; `wifi list` shows what was heard. Scan
//! results keep arriving while bmctl runs, through `poll()` from its step.
use crate::wifi::{self, pa, Iwl, CSR_FH_INT_STATUS, CSR_GP_CNTRL, CSR_INT, CSR_INT_MASK, CSR_RESET, GP_INIT_DONE};
use core::fmt::Write;

// ---- TX memory: the command queue's TFD ring, one idle ring shared by every other queue, byte-count tables ----
#[repr(C, align(256))]
struct Ring([[u8; 128]; 256]);
#[repr(C, align(1024))]
struct Bc([u8; 12800]);
#[repr(C, align(64))]
struct FirstTb([[u8; 64]; 32]);
#[repr(C, align(64))]
struct CmdBuf([[u8; 1536]; 32]);
static mut CMD_RING: Ring = Ring([[0; 128]; 256]);
static mut IDLE_RING: Ring = Ring([[0; 128]; 256]);
static mut BC: Bc = Bc([0; 12800]);
static mut FIRST_TB: FirstTb = FirstTb([[0; 64]; 32]);
static mut CMDS: CmdBuf = CmdBuf([[0; 1536]; 32]);
#[repr(C, align(64))]
struct Invalid([u8; 8]);
static INVALID: Invalid = Invalid([0x06, 0x0F, 0xFF, 0xFF, 0, 0, 0, 0]); // INVALID_WR_PTR_CMD filler (§6.1)

// ---- calibration results from INIT, re-sent to the runtime image (§9.1-9.2) ----
static mut CALIB: [([u8; 256], usize); 12] = [([0; 256], 0); 12];
static mut NCALIB: usize = 0;

pub struct Bss {
    pub ssid: [u8; 32],
    pub ssid_len: u8,
    pub bssid: [u8; 6],
    pub chan: u8,
    pub dbm: i8,
    pub privacy: bool,
    pub tsf: u64,
    pub bint: u16,
    pub capab: u16,
    pub rates: [u8; 16], // Supported + Extended Supported Rates, raw (bit 7 = basic)
    pub nrates: u8,
    pub rsn: [u8; 64], // the RSN IE body
    pub rsn_len: u8,
    pub dtim: u8,
}
const NBSS: usize = 128; // a crowded venue fills 32 slots on channels 1-4 alone (2026-10-22)
static mut BSS: [Option<Bss>; NBSS] = [const { None }; NBSS];

struct St {
    d: Iwl,
    q: u32,       // command queue (4, or 9 with IPAN)
    wp: u32,      // command ring write pointer 0..255
    mac: [u8; 6],
    tx_ant: u32,
    rx_ant: u32,
    up: bool,
    dwp: [u32; 2], // write pointers of data queues 0 and 2
    joined: bool,
    tk: [u8; 16],  // pairwise temporal key (CCMP), handed to every protected TX command
    open: bool,    // joined an open network: data goes unprotected
    pn: u64,       // CCMP packet number for our TX
    bssid: [u8; 6],
    scanning: bool,
    calib_done: bool,
    last_dbm: i8,
    events: u32,
    kck: [u8; 16], // PTK parts kept for the group key handshake (GTK rekey) after the join
    kek: [u8; 16],
    rc: [u8; 8],   // last EAPOL replay counter from the AP
    ssid: ([u8; 32], u8), // network and PMK kept for an automatic rejoin
    pmk: [u8; 32],
    lost: i64,     // 0 = fine; else when the AP dropped us (deauth, disassoc, pairwise rekey) -> rejoin
    gtk: [u8; 16], // installed group key (a retried group message 1 with the same key is only acknowledged)
    need_privacy: Option<bool>, // automatic rejoin: only a BSS with the original security (no downgrade to open)
}
static mut ST: Option<St> = None;

fn st() -> Option<&'static mut St> {
    unsafe { (*core::ptr::addr_of_mut!(ST)).as_mut() }
}

impl St {
    // ---- §2.1 / §3.1 / §3.4 ----
    fn prepare(&self) -> bool {
        let d = &self.d;
        let own = |d: &Iwl| {
            d.set(0x000, 0x0040_0000);
            d.poll(0x000, 0x0040_0000, 0x0040_0000, 50)
        };
        if own(d) {
            d.set(0x088, 0x20);
            return true;
        }
        d.set(0x250, 0x8000_0000);
        crate::wdt::delay_ms(2);
        for _ in 0..10 {
            d.set(0x000, 0x0800_0000);
            for _ in 0..750 {
                if own(d) {
                    d.set(0x088, 0x20);
                    return true;
                }
                crate::time::delay_us(200);
            }
            crate::wdt::pet();
            crate::wdt::delay_ms(25);
        }
        false
    }

    fn start_hw(&self) -> Result<(), &'static str> {
        if !self.prepare() {
            return Err("card not ready (prepare_card_hw)");
        }
        self.d.set(CSR_RESET, 0x80);
        crate::wdt::delay_ms(6);
        if !self.prepare() {
            return Err("card not ready after SW reset");
        }
        self.d.apm_init()?;
        self.d.wr(CSR_INT_MASK, 0x80);
        if self.d.rd(CSR_GP_CNTRL) & 0x0800_0000 == 0 {
            return Err("RF-kill switch is off");
        }
        Ok(())
    }

    fn stop(&self) {
        let d = &self.d;
        d.wr(CSR_INT_MASK, 0);
        d.wr(CSR_INT, 0xFFFF_FFFF);
        d.wr(CSR_FH_INT_STATUS, 0xFFFF_FFFF);
        if d.grab() {
            d.prph_wr(0xA02C10, 0);
            for ch in 0..8 {
                d.wr(0x1D00 + 0x20 * ch, 0);
            }
            let _ = d.poll(0x1EB0, 0x00FF_0000, 0x00FF_0000, 5000);
            d.wr(0x1C00, 0);
            let _ = d.poll(0x1C44, 0x0100_0000, 0x0100_0000, 1000);
            d.prph_wr(0x3008, 0x200);
            crate::time::delay_us(5);
            d.release();
        }
        d.clear(CSR_GP_CNTRL, 0x8);
        d.set(CSR_RESET, 0x200);
        let _ = d.poll(CSR_RESET, 0x100, 0x100, 100);
        d.set(CSR_RESET, 0x80);
        crate::wdt::delay_ms(5);
        d.clear(CSR_GP_CNTRL, GP_INIT_DONE);
        d.set(CSR_RESET, 0x80);
        crate::wdt::delay_ms(5);
        let _ = self.prepare();
        d.wr(CSR_INT_MASK, 0);
        d.wr(CSR_INT, 0xFFFF_FFFF);
        d.wr(CSR_FH_INT_STATUS, 0xFFFF_FFFF);
        d.wr(CSR_INT_MASK, 0x80);
    }

    // ---- §6.3 tx_init, §6.4 tx_start, §6.5 queue enable ----
    fn tx_init(&mut self) -> bool {
        unsafe {
            let dr = &mut *core::ptr::addr_of_mut!(DRING);
            let (d0, d1) = dr.split_at_mut(1);
            for r in [&mut *core::ptr::addr_of_mut!(CMD_RING), &mut *core::ptr::addr_of_mut!(IDLE_RING), &mut d0[0], &mut d1[0]] {
                for t in r.0.iter_mut() {
                    *t = [0; 128];
                    tfd_tb(t, 0, pa(&INVALID), 8);
                    t[3] = 1;
                }
            }
            (*core::ptr::addr_of_mut!(BC)).0 = [0; 12800];
        }
        self.wp = 0;
        self.dwp = [0; 2];
        let d = &self.d;
        if !d.grab() {
            return false;
        }
        d.prph_wr(0xA02C10, 0);
        d.wr(0x197C, pa(core::ptr::addr_of!(wifi::KW)) >> 4);
        for q in 0..20u32 {
            let ring = match q {
                _ if q == self.q => pa(core::ptr::addr_of!(CMD_RING)),
                0 => pa(unsafe { &(*core::ptr::addr_of!(DRING))[0] }),
                2 => pa(unsafe { &(*core::ptr::addr_of!(DRING))[1] }),
                _ => pa(core::ptr::addr_of!(IDLE_RING)),
            };
            let reg = if q < 16 { 0x19D0 + 4 * q } else { 0x1BF0 + 4 * (q - 16) };
            d.wr(reg, ring >> 8);
        }
        d.prph_set(0xA02DA8, 0x0004_0000);
        d.release();
        true
    }

    fn sram_write(&self, a: u32, vals: &[u32]) {
        self.d.wr(0x410, a);
        for &v in vals {
            self.d.wr(0x418, v);
        }
    }

    fn txq_enable(&self, scd_base: u32, q: u32, fifo: u32) {
        let d = &self.d;
        d.prph_wr(0xA02D0C + 4 * q, 0x0008_0000);
        if q != self.q {
            d.prph_set(0xA02CE8, 1 << q);
        }
        let v = d.prph_rd(0xA02E48);
        d.prph_wr(0xA02E48, v & !(1 << q));
        d.wr(0x460, q << 8);
        d.prph_wr(0xA02C68 + 4 * q, 0);
        self.sram_write(scd_base + 0x600 + 8 * q, &[0, 0x0040_0040]);
        d.prph_wr(0xA02D0C + 4 * q, 0x017F_0018 | fifo);
    }

    fn alive_notify(&mut self) -> Result<(), &'static str> {
        let d = &self.d;
        if !d.grab() {
            return Err("alive_notify: no NIC access");
        }
        let scd = d.prph_rd(0xA02C00);
        self.sram_write(scd + 0x600, &[0; 130]);
        d.prph_wr(0xA02C08, pa(core::ptr::addr_of!(BC)) >> 10);
        d.prph_wr(0xA02E44, 0);
        self.txq_enable(scd, self.q, 7);
        d.prph_wr(0xA02C10, 0xFF);
        for ch in 0..8 {
            d.wr(0x1D00 + 0x20 * ch, 0x8000_0008);
        }
        d.set(0x1E98, 0x2);
        let v = d.prph_rd(0x3010);
        d.prph_wr(0x3010, v & !0x800);
        // data queues -> FIFOs (§6.6)
        let map: &[(u32, u32)] = if self.q == 9 {
            &[(0, 3), (1, 2), (2, 1), (3, 0), (4, 0), (5, 4), (6, 2), (7, 5), (8, 4), (10, 5)]
        } else {
            &[(0, 3), (1, 2), (2, 1), (3, 0)]
        };
        for &(q, f) in map {
            self.txq_enable(scd, q, f);
        }
        d.release();
        self.cmd(0x5A, &[0; 68])?;
        // XTAL first, then everything stored (TEMP_OFFSET, the INIT results)
        let n = unsafe { NCALIB };
        for i in 0..n {
            let (b, l) = unsafe { (*core::ptr::addr_of!(CALIB))[i] };
            self.cmd(0xB0, &b[..l])?;
        }
        Ok(())
    }

    // ---- §6.7 host command, synchronous ----
    fn cmd(&mut self, id: u8, payload: &[u8]) -> Result<[u8; 64], &'static str> {
        let wp = self.wp;
        let slot = (wp & 31) as usize;
        let t = 4 + payload.len();
        if t > 1536 {
            return Err("command too long");
        }
        unsafe {
            let b = &mut (*core::ptr::addr_of_mut!(CMDS)).0[slot];
            b[0] = id;
            b[1] = 0;
            b[2..4].copy_from_slice(&((self.q & 0x1F) << 8 | wp).to_le_bytes()[..2]);
            b[4..t].copy_from_slice(payload);
            let ft = &mut (*core::ptr::addr_of_mut!(FIRST_TB)).0[slot];
            let n0 = t.min(20);
            ft[..n0].copy_from_slice(&b[..n0]);
            let tfd = &mut (*core::ptr::addr_of_mut!(CMD_RING)).0[wp as usize];
            *tfd = [0; 128];
            tfd_tb(tfd, 0, pa(ft), n0 as u32);
            tfd[3] = 1;
            if t > 20 {
                tfd_tb(tfd, 1, pa(&b[20]), (t - 20) as u32);
                tfd[3] = 2;
            }
        }
        self.wp = (wp + 1) & 255;
        self.d.wr(0x460, self.wp | self.q << 8);
        let want = ((self.q & 0x1F) << 8 | wp) as u16;
        for _ in 0..1000 {
            let mut got: Option<[u8; 64]> = None;
            self.rx_poll(&mut |cmd, seq, p| {
                if seq & 0x8000 == 0 && seq == want && !matches!(cmd, 0xC0 | 0xC1 | 0xC5 | 0x9D | 0x1C) {
                    let mut r = [0u8; 64];
                    let n = p.len().min(64);
                    r[..n].copy_from_slice(&p[..n]);
                    got = Some(r);
                }
            });
            if let Some(r) = got {
                return Ok(r);
            }
            if self.d.rd(CSR_INT) & (1 << 25 | 1 << 29) != 0 {
                return Err("firmware error (CSR_INT SW_ERR/HW_ERR)");
            }
            crate::wdt::pet();
            crate::wdt::delay_ms(1);
        }
        Err("no command response in 1 s")
    }

    // ---- §5.4 RX: every packet of every closed RB; notifications handled here ----
    fn rx_poll(&mut self, f: &mut dyn FnMut(u8, u16, &[u8])) {
        let closed = unsafe { (*core::ptr::addr_of!(wifi::RB_STATUS)).0[0] } as usize & 0xFFF & 255;
        let mut moved = false;
        loop {
            let r = unsafe { wifi::RX_READ };
            if r == closed {
                break;
            }
            let rb = unsafe { &(*core::ptr::addr_of!(wifi::RBS)).0[r % wifi::NRB] };
            let mut off = 0usize;
            while off + 8 < 4096 {
                let lnf = u32::from_le_bytes([rb[off], rb[off + 1], rb[off + 2], rb[off + 3]]);
                if lnf == 0x5555_0000 {
                    break;
                }
                let len = (lnf & 0x3FFF) as usize;
                let total = len + 4;
                let start = off;
                off += (total + 0x3F) & !0x3F;
                if total < 8 || off > 4096 {
                    break;
                }
                let cmd = rb[start + 4];
                let seq = u16::from_le_bytes([rb[start + 6], rb[start + 7]]);
                let p = &rb[start + 8..start + 4 + len];
                if cmd == 0x1C {
                    unsafe { TXSEQ = seq };
                }
                self.event(cmd, p);
                f(cmd, seq, p);
            }
            unsafe { wifi::RX_READ = (r + 1) % 256 };
            moved = true;
        }
        if moved {
            let r = unsafe { wifi::RX_READ };
            self.d.rx_kick(((r + wifi::NRB - 8) % 256) as u32);
            self.d.wr(CSR_FH_INT_STATUS, 0x4003_0000);
        }
    }

    fn event(&mut self, cmd: u8, p: &[u8]) {
        if cmd == 0x1C {
            // REPLY_TX: the sequence (queue, index) is in the packet header, read by tx_frame through TXSEQ
        }
        self.events = self.events.wrapping_add(1);
        match cmd {
            0x66 if !p.is_empty() && p.len() <= 256 => calib_store(p),
            0x67 => self.calib_done = true,
            0x84 => {
                self.scanning = false;
                crate::kprintln!("iwl: scan complete at {} ms", crate::time::now_ms());
            }
            0x1C if p.len() >= 40 => unsafe { TXST = Some((u16::from_le_bytes([p[32], p[33]]) as u8, u32::from_le_bytes([p[36], p[37], p[38], p[39]]))) },
            0xC0 if p.len() >= 36 => {
                let w = |o: usize| u32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]]);
                let agc = ((w(24) & 0xFE00) >> 9) as i32;
                let (v2, v3) = (w(28), w(32));
                let rssi = [(v2 & 0xFF) as i32, (v2 >> 16 & 0xFF) as i32, (v3 & 0xFF) as i32].into_iter().max().unwrap_or(0);
                self.last_dbm = (rssi - agc - 44).clamp(-127, 0) as i8;
            }
            0xC1 if p.len() >= 4 => {
                let l = u16::from_le_bytes([p[0], p[1]]) as usize;
                if l + 8 > p.len() || l < 24 {
                    return;
                }
                let st = u32::from_le_bytes([p[4 + l], p[5 + l], p[6 + l], p[7 + l]]);
                let fr = &p[4..4 + l];
                let fc = u16::from_le_bytes([fr[0], fr[1]]);
                if st & 3 != 3 {
                    return;
                }
                if !(fc & 0xFC == 0x80 || fc & 0xFC == 0x50) {
                    self.frame_in(fc, fr, st);
                    return;
                }
                if l < 36 {
                    return;
                }
                let mut b = Bss { ssid: [0; 32], ssid_len: 0, bssid: [0; 6], chan: 0, dbm: self.last_dbm, privacy: fr[34] & 0x10 != 0,
                    tsf: u64::from_le_bytes(fr[24..32].try_into().unwrap()), bint: u16::from_le_bytes([fr[32], fr[33]]),
                    capab: u16::from_le_bytes([fr[34], fr[35]]), rates: [0; 16], nrates: 0, rsn: [0; 64], rsn_len: 0, dtim: 1 };
                b.bssid.copy_from_slice(&fr[16..22]);
                if unsafe { WATCH } == Some(b.bssid) && fc & 0xFC == 0x80 {
                    unsafe { SEEN_TSF = Some(b.tsf) };
                }
                let mut i = 36;
                while i + 2 <= fr.len() {
                    let (id, n) = (fr[i], fr[i + 1] as usize);
                    if i + 2 + n > fr.len() {
                        break;
                    }
                    let v = &fr[i + 2..i + 2 + n];
                    if id == 0 && n <= 32 {
                        b.ssid[..n].copy_from_slice(v);
                        b.ssid_len = n as u8;
                    } else if id == 3 && n == 1 {
                        b.chan = v[0];
                    } else if id == 1 || id == 50 {
                        for &r in v {
                            if (b.nrates as usize) < 16 {
                                b.rates[b.nrates as usize] = r;
                                b.nrates += 1;
                            }
                        }
                    } else if id == 48 && n <= 64 {
                        b.rsn[..n].copy_from_slice(v);
                        b.rsn_len = n as u8;
                    } else if id == 5 && n >= 2 {
                        b.dtim = v[1].max(1);
                    }
                    i += 2 + n;
                }
                bss_add(b);
            }
            _ => {}
        }
    }

    // ---- §7.2-7.4 start a firmware image and wait for its ALIVE ----
    fn start_fw(&mut self, init: bool, w: &mut dyn Write) -> Result<(), &'static str> {
        let d = &self.d;
        d.wr(CSR_INT_MASK, 0x80);
        d.wr(CSR_INT, 0xFFFF_FFFF);
        d.wr(CSR_INT_MASK, 0);
        d.wr(CSR_INT, 0xFFFF_FFFF);
        d.wr(CSR_FH_INT_STATUS, 0xFFFF_FFFF);
        d.wr(0x05C, 0x2);
        d.wr(0x05C, 0x4);
        d.wr(CSR_INT, 0xFFFF_FFFF);
        d.apm_init()?;
        d.nic_config();
        if !d.rx_init() {
            return Err("rx init");
        }
        if !self.tx_init() {
            return Err("tx init");
        }
        let d = &self.d;
        d.wr(CSR_INT_MASK, 0x0800_0000);
        d.wr(0x05C, 0x2);
        d.wr(0x05C, 0x2);
        let (ti, td) = if init { (3, 4) } else { (1, 2) };
        let inst = wifi::tlvs().find(|t| t.0 == ti).ok_or("no inst TLV")?.1;
        let data = wifi::tlvs().find(|t| t.0 == td).ok_or("no data TLV")?.1;
        d.load_section(0x80_0000, data)?;
        d.load_section(0x0, inst)?;
        d.wr(CSR_INT_MASK, 0xBA00_008F);
        d.wr(CSR_RESET, 0);
        for _ in 0..1000 {
            let mut alive = None;
            self.rx_poll(&mut |cmd, _, p| {
                if cmd == 1 && p.len() >= 32 {
                    alive = Some(u32::from_le_bytes([p[28], p[29], p[30], p[31]]));
                }
            });
            match alive {
                Some(1) => {
                    let _ = write!(w, "{} alive; ", if init { "INIT" } else { "runtime" });
                    crate::wdt::delay_ms(5);
                    return Ok(());
                }
                Some(_) => return Err("ALIVE not valid"),
                None => {}
            }
            crate::wdt::pet();
            crate::wdt::delay_ms(1);
        }
        Err("no ALIVE in 1 s")
    }
}

fn tfd_tb(t: &mut [u8; 128], i: usize, addr: u32, len: u32) {
    let o = 4 + 6 * i;
    t[o..o + 4].copy_from_slice(&addr.to_le_bytes());
    t[o + 4..o + 6].copy_from_slice(&((len << 4) as u16).to_le_bytes());
}

fn calib_store(p: &[u8]) {
    unsafe {
        let c = &mut *core::ptr::addr_of_mut!(CALIB);
        let n = &mut *core::ptr::addr_of_mut!(NCALIB);
        let i = (0..*n).find(|&i| c[i].0[0] == p[0]).unwrap_or(*n);
        if i < c.len() {
            c[i].0[..p.len()].copy_from_slice(p);
            c[i].1 = p.len();
            if i == *n {
                *n += 1;
            }
        }
    }
}

fn bss_add(b: Bss) {
    let l = unsafe { &mut *core::ptr::addr_of_mut!(BSS) };
    if let Some(e) = l.iter_mut().flatten().find(|e| e.bssid == b.bssid) {
        if b.dbm > e.dbm {
            e.dbm = b.dbm;
        }
        return;
    }
    if !b.ssid[..b.ssid_len as usize].iter().any(|&c| c > b' ') {
        return; // hidden networks cannot be picked; they only took slots
    }
    if let Some(s) = l.iter_mut().find(|e| e.is_none()) {
        *s = Some(b);
    } else if let Some(w) = l.iter_mut().min_by_key(|e| e.as_ref().map_or(i8::MIN, |e| e.dbm)) {
        if w.as_ref().is_some_and(|w| w.dbm < b.dbm) {
            *w = Some(b); // full: the weakest makes room for a stronger one
        }
    }
}

/// The whole bring-up (§11 steps 1-11). Reports progress into `w`.
fn up(w: &mut dyn Write) -> Result<(), &'static str> {
    let d = wifi::probe()?;
    let mut s = St { d, q: 4, wp: 0, mac: [0; 6], tx_ant: 1, rx_ant: 3, up: false, dwp: [0; 2], joined: false, tk: [0; 16], open: false, pn: 0, bssid: [0; 6], scanning: false, calib_done: false, last_dbm: -100, events: 0,
        kck: [0; 16], kek: [0; 16], rc: [0; 8], ssid: ([0; 32], 0), pmk: [0; 32], lost: 0,
        gtk: [0; 16], need_privacy: None };
    s.start_hw()?;
    let _ = s.d.read_nvm()?;
    let n = unsafe { &*core::ptr::addr_of!(wifi::NVM) };
    let w16 = |o: usize| u16::from_le_bytes([n[o], n[o + 1]]) as u32;
    s.mac.copy_from_slice(&n[0x2A..0x30]);
    let rc = w16(0x90);
    s.tx_ant = (rc >> 8 & 0xF).max(1);
    s.rx_ant = (rc >> 12 & 0xF).max(1);
    s.q = if w16(0x8A) & 0x100 != 0 { 9 } else { 4 };
    let cal = (w16(0xCE) << 1) as usize;
    let (x0, x1) = if cal + 0x254 <= 2048 { (n[cal + 0x250], n[cal + 0x252]) } else { (0, 0) };
    let raw_t = if cal + 0x258 <= 2048 { w16(cal + 0x256) } else { 0 };
    let _ = write!(w, "MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} tx_ant {:#x} rx_ant {:#x} cmd_q {} xtal {}/{} temp {}; ",
        s.mac[0], s.mac[1], s.mac[2], s.mac[3], s.mac[4], s.mac[5], s.tx_ant, s.rx_ant, s.q, x0, x1, raw_t);
    unsafe {
        NCALIB = 0;
        *core::ptr::addr_of_mut!(BSS) = [const { None }; NBSS];
    }
    calib_store(&[0x0F, 0, 1, 1, x0, x1, 0, 0]); // XTAL (§9.0)
    s.stop();
    s.start_hw()?;
    // INIT image: alive_notify, BT env open, calibration config, collect results until CALIBRATION_COMPLETE
    s.start_fw(true, w)?;
    s.alive_notify()?;
    s.cmd(0xCD, &[1, 1, 0, 0])?;
    let mut cfg = [0u8; 92];
    for o in [0, 4, 8] {
        cfg[o..o + 4].copy_from_slice(&0x5Fu32.to_le_bytes());
    }
    cfg[40] = 1;
    s.cmd(0x65, &cfg)?;
    let t = if raw_t == 0 { 2700 } else { raw_t } as u16;
    calib_store(&[0x12, 0, 1, 1, t as u8, (t >> 8) as u8, 0, 0]); // TEMP_OFFSET v1 (stored, sent at runtime)
    for _ in 0..2000 {
        s.rx_poll(&mut |_, _, _| {});
        if s.calib_done {
            break;
        }
        crate::wdt::pet();
        crate::wdt::delay_ms(1);
    }
    let _ = write!(w, "calibration {} ({} results); ", if s.calib_done { "complete" } else { "TIMEOUT" }, unsafe { NCALIB });
    s.stop();
    s.start_hw()?;
    s.start_fw(false, w)?;
    s.alive_notify()?;
    // runtime configuration (§9.3)
    let mut bt = [0u8; 72];
    bt[0] = 0x91;
    bt[2] = 5;
    bt[3] = 1;
    bt[4..8].copy_from_slice(&0xFFFF_0000u32.to_le_bytes());
    bt[8..12].copy_from_slice(&0xFFFF_0000u32.to_le_bytes());
    bt[12] = 2;
    bt[13] = 0x0C;
    let lut: [u32; 12] = [0xAAAAAAAA, 0xAAAAAAAA, 0xAEAAAAAA, 0xAAAAAAAA, 0xCC00FF28, 0x0000AAAA, 0xCC00AAAA, 0x0000AAAA, 0xC0004000, 0x00004000, 0xF0005000, 0xF0005000];
    for (i, v) in lut.iter().enumerate() {
        bt[16 + 4 * i..20 + 4 * i].copy_from_slice(&v.to_le_bytes());
    }
    bt[66..68].copy_from_slice(&0x00FFu16.to_le_bytes());
    bt[68] = 0xF0;
    s.cmd(0x9B, &bt)?;
    s.cmd(0xCC, &[6, 7, 2, 3, 4, 5, 6, 8, 0x0A, 0, 0, 0, 0, 0, 0, 0])?;
    s.cmd(0xCD, &[1, 1, 0, 0])?;
    s.cmd(0xCD, &[0, 1, 0, 0])?;
    let mut cfg = [0u8; 92];
    cfg[0..4].copy_from_slice(&0x5DFu32.to_le_bytes());
    cfg[4..8].copy_from_slice(&2u32.to_le_bytes());
    s.cmd(0x65, &cfg)?;
    s.cmd(0x98, &s.tx_ant.to_le_bytes())?;
    s.commit_rxon(false)?;
    s.d.wr(0x05C, 0x8);
    let mut ct = [0u8; 12];
    ct[0..4].copy_from_slice(&95u32.to_le_bytes());
    ct[8..12].copy_from_slice(&114u32.to_le_bytes());
    s.cmd(0xA4, &ct)?;
    let mut pw = [0u8; 36];
    pw[0] = 0x08; // CAM + PCI_PM
    s.cmd(0x77, &pw)?;
    s.commit_rxon(true)?;
    s.cmd(0x95, &[28, 0x40, 0x7F, 0])?; // 14 dBm global limit (½ dBm units); the NVM enhanced table is not parsed yet
    s.up = true;
    let _ = write!(w, "up: ready to scan ({} rx packets)", s.events);
    unsafe { *core::ptr::addr_of_mut!(ST) = Some(s) };
    Ok(())
}

impl St {
    /// §9.5: RXON (with or without our MAC), broadcast station 15, its link quality.
    fn commit_rxon(&mut self, with_mac: bool) -> Result<(), &'static str> {
        let mut r = [0u8; 50];
        if with_mac {
            r[0..6].copy_from_slice(&self.mac);
        }
        r[24] = 3;
        let n = self.rx_ant.count_ones();
        let chain = (self.rx_ant << 1) | (n.min(2) << 12) | (n.min(1) << 10);
        r[26..28].copy_from_slice(&(chain as u16).to_le_bytes());
        r[28] = 0x15;
        r[29] = 0x0F;
        r[32..36].copy_from_slice(&0x0000_8005u32.to_le_bytes());
        if with_mac {
            r[36] = 0x04;
        }
        r[40] = 1;
        r[42] = 0xFF;
        r[43] = 0xFF;
        r[44] = 0xFF;
        self.cmd(0x10, &r)?;
        let mut sta = [0u8; 92];
        sta[4..10].copy_from_slice(&[0xFF; 6]);
        sta[12] = 15;
        let resp = self.cmd(0x18, &sta)?;
        if resp[0] != 1 {
            return Err("ADD_STA broadcast refused");
        }
        let first = self.tx_ant & self.tx_ant.wrapping_neg();
        let mut lq = [0u8; 88];
        lq[0] = 15;
        lq[6] = first as u8;
        lq[7] = { let d = self.tx_ant & !first; if d == 0 { 3 } else { d } } as u8;
        lq[12..14].copy_from_slice(&4000u16.to_le_bytes());
        lq[14] = 3;
        let rate = 0x20A | (first << 14);
        for i in 0..16 {
            lq[20 + 4 * i..24 + 4 * i].copy_from_slice(&rate.to_le_bytes());
        }
        self.cmd(0x4E, &lq)?;
        Ok(())
    }

    /// §10.1: a passive scan of 2.4 GHz channels 1-13 (nothing is transmitted).
    fn scan(&mut self) -> Result<(), &'static str> {
        let mut c = [0u8; 1100];
        let mut probe = [0u8; 36];
        probe[0] = 0x40;
        probe[4..10].copy_from_slice(&[0xFF; 6]);
        probe[10..16].copy_from_slice(&self.mac);
        probe[16..22].copy_from_slice(&[0xFF; 6]);
        probe[24..26].copy_from_slice(&[0, 0]);
        probe[26..36].copy_from_slice(&[1, 8, 0x02, 0x04, 0x0B, 0x16, 0x0C, 0x12, 0x18, 0x24]);
        let nch = 13usize;
        let len = 764 + probe.len() + 12 * nch;
        c[0..2].copy_from_slice(&(len as u16).to_le_bytes());
        c[3] = nch as u8;
        c[4..6].copy_from_slice(&10u16.to_le_bytes());
        c[6..8].copy_from_slice(&1u16.to_le_bytes());
        let ra = self.rx_ant;
        let chain = 1 | (ra << 4) | (ra << 7) | (self.rx_ant << 1);
        c[10..12].copy_from_slice(&(chain as u16).to_le_bytes());
        c[20..24].copy_from_slice(&5u32.to_le_bytes());
        c[24..28].copy_from_slice(&0x44u32.to_le_bytes());
        let tx = 28;
        c[tx..tx + 2].copy_from_slice(&(probe.len() as u16).to_le_bytes());
        c[tx + 4..tx + 8].copy_from_slice(&0x3000u32.to_le_bytes());
        let first = self.tx_ant & self.tx_ant.wrapping_neg();
        c[tx + 12..tx + 16].copy_from_slice(&(0x20A | first << 14).to_le_bytes()); // 1 Mbps CCK on the first antenna
        c[tx + 16] = 15;
        c[tx + 40..tx + 44].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        c[764..764 + probe.len()].copy_from_slice(&probe);
        for i in 0..nch {
            let o = 764 + probe.len() + 12 * i;
            c[o + 4..o + 6].copy_from_slice(&((i + 1) as u16).to_le_bytes()); // type 0 = passive
            c[o + 6] = 0x28;
            c[o + 7] = 110;
            c[o + 8..o + 10].copy_from_slice(&33u16.to_le_bytes());
            c[o + 10..o + 12].copy_from_slice(&120u16.to_le_bytes());
        }
        self.scanning = true;
        crate::kprintln!("iwl: scan start at {} ms", crate::time::now_ms());
        let r = self.cmd(0x80, &c[..len])?;
        let status = u32::from_le_bytes([r[0], r[1], r[2], r[3]]);
        if status != 1 {
            self.scanning = false;
            return Err("scan refused by firmware");
        }
        Ok(())
    }
}

/// Keep draining RX while the radio is up (scan results arrive between bmctl commands).
pub fn poll() {
    if let Some(s) = st() {
        if s.up {
            s.rx_poll(&mut |_, _, _| {});
        }
        link_check(s);
    }
    net_poll();
}

/// After a join: answer EAPOL from the AP (GTK rekey) and rejoin when it dropped us. Called from bmctl's poll and
/// from the WiFi device itself, so it also runs during an SSH session over WiFi.
fn link_check(s: &mut St) {
    if s.joined {
        let e = unsafe { &mut *core::ptr::addr_of_mut!(EAPOL) };
        if e.1 > 0 {
            let (f, n) = (e.0, e.1);
            e.1 = 0;
            s.eapol_after_join(&f[..n]);
        }
        let m = unsafe { &mut *core::ptr::addr_of_mut!(MGMT) };
        if m.1 >= 26 && matches!(m.0[0] & 0xFC, 0xC0 | 0xA0) && m.0[16..22] == s.bssid {
            crate::kprintln!("wifi: {} by the AP, reason {} -> rejoin", if m.0[0] & 0xFC == 0xC0 { "deauthenticated" } else { "disassociated" }, m.0[24]);
            m.1 = 0;
            s.lost = crate::time::now_ms();
        }
    }
    let t = crate::time::now_ms();
    if s.lost != 0 && s.ssid.1 > 0 && t - s.lost > 1000 {
        s.lost = t + 9000; // next try in >= 10 s if this one fails
        s.joined = false;
        let (ssid, n, pmk) = (s.ssid.0, s.ssid.1 as usize, s.pmk);
        crate::kprint!("wifi: rejoin: ");
        s.need_privacy = Some(!s.open);
        let r = s.join(&ssid[..n], &pmk, &mut Sink);
        s.need_privacy = None;
        match r {
            Ok(()) => {
                s.lost = 0;
                crate::kprintln!(" (address kept: {:?})", unsafe { W_IP });
            }
            Err(e) => crate::kprintln!(" failed: {}", e),
        }
    }
}

/// The GTK KDE (00-0F-AC:1) in unwrapped key data of length `n`: (key, key id).
fn parse_gtk(kd: &[u8; 256], n: usize) -> Option<([u8; 16], u8)> {
    let mut j = 0;
    while j + 2 <= n {
        let (id, l) = (kd[j], kd[j + 1] as usize);
        if j + 2 + l > n || (l == 0 && id == 0xDD) {
            return None;
        }
        if id == 0xDD && l >= 6 + 16 && kd[j + 2..j + 6] == [0, 0x0F, 0xAC, 1] {
            let mut g = [0u8; 16];
            g.copy_from_slice(&kd[j + 8..j + 24]);
            return Some((g, kd[j + 6] & 3));
        }
        j += 2 + l;
    }
    None
}

pub fn cmd<'a>(w: &mut dyn Write, sub: &str, _it: &mut impl Iterator<Item = &'a str>) {
    match sub {
        "up" => {
            if let Err(e) = up(w) {
                let _ = write!(w, " err {}", e);
            }
        }
        "scan" => match st() {
            Some(s) if s.up => match s.scan() {
                Ok(()) => {
                    let _ = write!(w, "scanning 2.4 GHz channels 1-13 (passive, ~2 s): wifi list");
                }
                Err(e) => {
                    let _ = write!(w, "err {}", e);
                }
            },
            _ => {
                let _ = write!(w, "err not up (wifi up)");
            }
        },
        "list" => {
            poll();
            let sc = st().map_or(false, |s| s.scanning);
            let _ = write!(w, "{}", if sc { "(scan running) " } else { "" });
            for b in unsafe { (*core::ptr::addr_of!(BSS)).iter().flatten() } {
                let name = core::str::from_utf8(&b.ssid[..b.ssid_len as usize]).unwrap_or("?");
                let _ = write!(w, "[{}{}] ch {} {} dBm {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}; ", if name.is_empty() { "<hidden>" } else { name },
                    if b.privacy { " (secured)" } else { "" }, b.chan, b.dbm, b.bssid[0], b.bssid[1], b.bssid[2], b.bssid[3], b.bssid[4], b.bssid[5]);
            }
        }
        "join" => join_cmd(w, _it),
        "dhcp" => dhcp_cmd(w),
        "stats" => {
            let s = unsafe { WSTAT };
            let _ = write!(w, "data in {} ccmp ok {} decrypt refused {} (last rx status {:#x}) to IP {} from IP {} last tx status {:#x}", s[0], s[1], s[2], s[3], s[4], s[5], s[6]);
        }
        _ => {
            let _ = write!(w, "err wifi up|scan|list|join");
        }
    }
}

// ======================= association + WPA2-PSK (docs/iwl-dvm-associate.md) =======================

// Data queues 0 (management, VO) and 2 (BE data): own TFD rings; one frame in flight at a time, so 32 buffer slots.
static mut DRING: [Ring; 2] = [const { Ring([[0; 128]; 256]) }; 2];
static mut DFT: [FirstTb; 2] = [const { FirstTb([[0; 64]; 32]) }; 2];
#[repr(C, align(64))]
struct TxCmds([[u8; 128]; 32]);
static mut DCMD: [TxCmds; 2] = [const { TxCmds([[0; 128]; 32]) }; 2];
#[repr(C, align(64))]
struct Bodies([[u8; 1600]; 32]);
static mut DBODY: [Bodies; 2] = [const { Bodies([[0; 1600]; 32]) }; 2];
static mut TXSEQ: u16 = 0xFFFF; // sequence of the last REPLY_TX
static mut TXST: Option<(u8, u32)> = None; // its (status, scd_ssn)
static mut WATCH: Option<[u8; 6]> = None; // BSSID whose beacon we wait for
static mut SEEN_TSF: Option<u64> = None;
static mut MGMT: ([u8; 512], usize) = ([0; 512], 0); // last management frame addressed to us
static mut EAPOL: ([u8; 512], usize) = ([0; 512], 0); // last EAPOL PDU from the AP

fn store(dst: &mut ([u8; 512], usize), src: &[u8]) {
    let n = src.len().min(512);
    dst.0[..n].copy_from_slice(&src[..n]);
    dst.1 = n;
}

/// Our RSN IE: WPA2-PSK, pairwise CCMP; the group cipher is copied from the AP (byte-identical in the assoc
/// request and EAPOL message 2, §3.2 / §5.4).
fn our_rsn(ap: &Bss) -> [u8; 22] {
    let mut r = [0x30, 0x14, 1, 0, 0x00, 0x0F, 0xAC, 4, 1, 0, 0x00, 0x0F, 0xAC, 4, 1, 0, 0x00, 0x0F, 0xAC, 2, 0, 0];
    if ap.rsn_len >= 6 {
        r[4..8].copy_from_slice(&ap.rsn[2..6]);
    }
    r
}

impl St {
    /// Non-beacon frames: management frames to us, EAPOL from the AP (§7.2; data path to IP comes later).
    fn frame_in(&mut self, fc: u16, fr: &[u8], _st: u32) {
        if fr.len() < 24 || (fr[4..10] != self.mac && fr[4] & 1 == 0) {
            return;
        }
        match fc & 0x0C {
            0x00 if fr[4..10] == self.mac => unsafe { store(&mut *core::ptr::addr_of_mut!(MGMT), fr) },
            0x08 if fc & 0x0040 == 0 => {
                unsafe { WSTAT[0] += 1 };
                let hl = if fc & 0x0080 != 0 { 26 } else { 24 };
                if fr.len() < hl + 8 || fr[16..22] == self.mac {
                    return; // too short, or our own broadcast reflected by the AP
                }
                let b = if fc & 0x4000 != 0 {
                    // hardware-decrypted CCMP: header and MIC still in the frame (§7.1)
                    if _st & 0x700 != 0x200 || _st & 0x800 == 0 || _st & 0x40 == 0 || fr.len() < hl + 16 + 8 {
                        unsafe {
                            WSTAT[2] += 1;
                            WSTAT[3] = _st;
                        }
                        return;
                    }
                    let h = &fr[hl..hl + 8];
                    let pn = u64::from_le_bytes([h[0], h[1], h[4], h[5], h[6], h[7], 0, 0]);
                    let slot = unsafe { &mut (*core::ptr::addr_of_mut!(RX_PN))[(fr[4] & 1) as usize] };
                    if pn <= *slot && !(pn == 0 && *slot == 0) {
                        return; // CCMP replay (the firmware leaves the PN check to the host)
                    }
                    *slot = pn;
                    unsafe { WSTAT[1] += 1 };
                    &fr[hl + 8..fr.len() - 8]
                } else {
                    &fr[hl..]
                };
                if b.len() < 8 || b[..6] != [0xAA, 0xAA, 3, 0, 0, 0] {
                    return;
                }
                if b[6..8] == [0x88, 0x8E] {
                    unsafe { store(&mut *core::ptr::addr_of_mut!(EAPOL), &b[8..]) };
                } else if self.joined && (fc & 0x4000 != 0 || self.open) {
                    eth_push(&fr[4..10], &fr[16..22], &b[6..]); // dst, src (from-DS: addr3), ethertype + payload
                    unsafe { WSTAT[4] += 1 };
                }
            }
            _ => {}
        }
    }

    /// REPLY_TX on data queue q (0 or 2), §1.2-1.7; waits for the TX status. Returns its status byte.
    fn tx_frame(&mut self, q: u32, frame: &[u8], hdr_len: usize, sta: u8, flags: u32, rate: u32, tid: u8, pm_to: u16) -> Result<u8, &'static str> {
        self.tx_frame_k(q, frame, hdr_len, sta, flags, rate, tid, pm_to, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn tx_frame_k(&mut self, q: u32, frame: &[u8], hdr_len: usize, sta: u8, flags: u32, rate: u32, tid: u8, pm_to: u16, key: Option<[u8; 16]>) -> Result<u8, &'static str> {
        let qi = if q == 0 { 0 } else { 1 };
        let wp = self.dwp[qi];
        let slot = (wp & 31) as usize;
        if frame.len() > 1600 || hdr_len > frame.len() {
            return Err("frame too long");
        }
        let seq = (q << 8 | wp) as u16;
        let tb1 = (56 + 4 + hdr_len - 20 + 3) & !3;
        let flags = if tb1 != 40 + hdr_len { flags | 0x0010_0000 } else { flags };
        unsafe {
            let c = &mut (*core::ptr::addr_of_mut!(DCMD))[qi].0[slot];
            let ft = &mut (*core::ptr::addr_of_mut!(DFT))[qi].0[slot];
            let body = &mut (*core::ptr::addr_of_mut!(DBODY))[qi].0[slot];
            *c = [0; 128];
            c[0] = 0x1C;
            c[2..4].copy_from_slice(&seq.to_le_bytes());
            c[4..6].copy_from_slice(&(frame.len() as u16).to_le_bytes());
            c[8..12].copy_from_slice(&flags.to_le_bytes());
            c[16..20].copy_from_slice(&rate.to_le_bytes());
            c[20] = sta;
            if let Some(k) = key {
                c[21] = 0x02; // TX_CMD_SEC_CCM: the device appends the MIC
                c[24..40].copy_from_slice(&k);
            }
            c[44..48].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
            c[48..52].copy_from_slice(&(pa(ft) + 12).to_le_bytes());
            c[53] = 0x3C;
            c[54] = 0x0F;
            c[55] = tid;
            c[56..58].copy_from_slice(&pm_to.to_le_bytes());
            c[60..60 + hdr_len].copy_from_slice(&frame[..hdr_len]);
            let bl = frame.len() - hdr_len;
            body[..bl].copy_from_slice(&frame[hdr_len..]);
            let tfd = &mut (*core::ptr::addr_of_mut!(DRING))[qi].0[wp as usize];
            *tfd = [0; 128];
            tfd_tb(tfd, 0, pa(ft), 20);
            tfd_tb(tfd, 1, pa(&c[20]), tb1 as u32);
            tfd[3] = 2;
            if bl > 0 {
                tfd_tb(tfd, 2, pa(body), bl as u32);
                tfd[3] = 3;
            }
            ft[..20].copy_from_slice(&c[..20]); // TB0 is a copy, made last (§1.3)
            let bc = &mut (*core::ptr::addr_of_mut!(BC)).0;
            let e = ((frame.len() as u16 + 8 + if key.is_some() { 8 } else { 0 }) | (sta as u16) << 12).to_le_bytes();
            let i = 2 * (q as usize * 320 + wp as usize);
            bc[i..i + 2].copy_from_slice(&e);
            if wp < 64 {
                bc[i + 512..i + 514].copy_from_slice(&e);
            }
            TXST = None;
            TXSEQ = 0xFFFF;
        }
        self.dwp[qi] = (wp + 1) & 255;
        self.d.wr(0x460, self.dwp[qi] | q << 8);
        for _ in 0..500 {
            self.rx_poll(&mut |_, _, _| {});
            if let (Some((stt, _)), true) = (unsafe { TXST }, unsafe { TXSEQ } == seq) {
                return Ok(stt);
            }
            crate::wdt::pet();
            crate::wdt::delay_ms(1);
        }
        Err("no TX status in 500 ms")
    }

    fn add_sta(&mut self, id: u8, addr: &[u8; 6]) -> Result<(), &'static str> {
        let mut sta = [0u8; 92];
        sta[4..10].copy_from_slice(addr);
        sta[12] = id;
        if self.cmd(0x18, &sta)?[0] != 1 {
            return Err("ADD_STA refused");
        }
        Ok(())
    }

    fn lq(&mut self, id: u8) -> Result<(), &'static str> {
        let first = self.tx_ant & self.tx_ant.wrapping_neg();
        let mut lq = [0u8; 88];
        lq[0] = id;
        lq[6] = first as u8;
        lq[7] = { let d = self.tx_ant & !first; if d == 0 { 3 } else { d } } as u8;
        lq[12..14].copy_from_slice(&4000u16.to_le_bytes());
        lq[14] = 3;
        if id == 0 {
            lq[15] = 0x3F;
        }
        for i in 0..16 {
            lq[20 + 4 * i..24 + 4 * i].copy_from_slice(&(0x20A | first << 14).to_le_bytes());
        }
        self.cmd(0x4E, &lq)?;
        Ok(())
    }

    fn rxon_bss(&mut self, b: &Bss, aid: u16, filter: u32) -> Result<(), &'static str> {
        self.bssid = b.bssid; // tuned to a BSS: the next scan restarts the radio (scan_start)
        let mut r = [0u8; 50];
        r[0..6].copy_from_slice(&self.mac);
        r[8..14].copy_from_slice(&b.bssid);
        r[24] = 3;
        let n = self.rx_ant.count_ones();
        r[26..28].copy_from_slice(&(((self.rx_ant << 1) | (n.min(2) << 12) | (n.min(1) << 10)) as u16).to_le_bytes());
        // basic rates (§2.3)
        let (mut cck, mut ofdm) = (0u8, 0u8);
        let table = [0x02u8, 0x04, 0x0B, 0x16, 0x0C, 0x12, 0x18, 0x24, 0x30, 0x48, 0x60, 0x6C];
        for &rt in &b.rates[..b.nrates as usize] {
            if rt & 0x80 != 0 {
                if let Some(i) = table.iter().position(|&t| t == rt & 0x7F) {
                    if i < 4 { cck |= 1 << i } else { ofdm |= 1 << (i - 4) }
                }
            }
        }
        if ofdm & 0x0F == 0 { ofdm |= 0x10 }
        if ofdm & 0x03 == 0 { ofdm |= 0x04 }
        ofdm |= 0x01;
        if cck & 0x07 == 0 { cck |= 0x08 }
        if cck & 0x03 == 0 { cck |= 0x04 }
        if cck & 0x01 == 0 { cck |= 0x02 }
        cck |= 0x01;
        r[28] = ofdm;
        r[29] = cck;
        r[30..32].copy_from_slice(&aid.to_le_bytes());
        let mut flags = 0x0000_8005u32;
        if aid != 0 {
            if b.capab & 0x0400 != 0 { flags |= 0x10 }
            if b.capab & 0x0020 != 0 { flags |= 0x20 }
        }
        r[32..36].copy_from_slice(&flags.to_le_bytes());
        r[36..40].copy_from_slice(&filter.to_le_bytes());
        r[40..42].copy_from_slice(&(b.chan as u16).to_le_bytes());
        r[42] = 0xFF;
        r[43] = 0xFF;
        r[44] = 0xFF;
        self.cmd(0x10, &r)?;
        Ok(())
    }

    fn wait_mgmt(&mut self, fc: u16, ms: u32) -> Option<([u8; 512], usize)> {
        for _ in 0..ms {
            self.rx_poll(&mut |_, _, _| {});
            let m = unsafe { &mut *core::ptr::addr_of_mut!(MGMT) };
            if m.1 >= 24 {
                let f = u16::from_le_bytes([m.0[0], m.0[1]]);
                if f & 0xFC == fc as u16 & 0xFC || f & 0xFC == 0xC0 || f & 0xFC == 0xA0 {
                    let r = (m.0, m.1);
                    m.1 = 0;
                    return Some(r);
                }
                m.1 = 0;
            }
            crate::wdt::pet();
            crate::wdt::delay_ms(1);
        }
        None
    }

    fn install_key(&mut self, bssid: &[u8; 6], flags: u16, off: u8, key: &[u8]) -> Result<(), &'static str> {
        let mut k = [0u8; 92];
        k[0] = 1;
        k[4..10].copy_from_slice(bssid);
        k[13] = 1;
        k[16..18].copy_from_slice(&flags.to_le_bytes());
        k[30] = off;
        k[32..48].copy_from_slice(key);
        if self.cmd(0x18, &k)?[0] != 1 {
            return Err("key install refused");
        }
        Ok(())
    }

    /// EAPOL from the AP after the join: group key handshake message 1 (GTK rekey) -> new GTK + message 2; a pairwise
    /// message 1 (PTK rekey) -> rejoin. Without this the AP drops us at its GTK rekey (deauth reason 2, 2026-10-22).
    fn eapol_after_join(&mut self, m: &[u8]) {
        if m.len() < 99 || m[1] != 3 {
            return;
        }
        let ki = u16::from_be_bytes([m[5], m[6]]);
        if ki & 0x0008 != 0 {
            // only a fresh pairwise message 1 (Ack, no MIC); the first indication latches the rejoin time
            if ki & 0x0180 == 0x0080 && m[9..17] > self.rc[..] && self.lost == 0 {
                crate::kprintln!("wifi: AP started a new 4-way handshake -> rejoin");
                self.lost = crate::time::now_ms();
            }
            return;
        }
        if ki & 0x0180 != 0x0180 || m[9..17] < self.rc[..] {
            return; // not a group message 1 (MIC + Ack), or a replay
        }
        let kdl = u16::from_be_bytes([m[97], m[98]]) as usize;
        if 99 + kdl > m.len() || kdl < 24 || kdl > 264 {
            return;
        }
        let mut chk = [0u8; 512];
        chk[..99 + kdl].copy_from_slice(&m[..99 + kdl]);
        chk[81..97].fill(0);
        if crate::wpa::mic(&self.kck, &chk[..99 + kdl]) != m[81..97] {
            crate::kprintln!("wifi: group key message with a wrong MIC ignored");
            return;
        }
        let mut kd = [0u8; 256];
        if !crate::wpa::aes_unwrap(&self.kek, &m[99..99 + kdl], &mut kd) {
            return;
        }
        let Some((gtk, gid)) = parse_gtk(&kd, kdl - 8) else { return };
        let bssid = self.bssid;
        // a retry (same counter, or the key we already have): acknowledge again, never reinstall or reset its PN
        let fresh = m[9..17] > self.rc[..] && gtk != self.gtk;
        if fresh {
            if self.install_key(&bssid, 0x400A | (gid as u16) << 8, 1, &gtk).is_err() {
                self.lost = crate::time::now_ms();
                return;
            }
            self.gtk = gtk;
            let mut rsc = [0u8; 8];
            rsc[..6].copy_from_slice(&m[65..71]);
            unsafe { RX_PN[1] = u64::from_le_bytes(rsc) };
        }
        self.rc.copy_from_slice(&m[9..17]);
        let mut m2 = [0u8; 99];
        m2[0] = m[0];
        m2[1] = 3;
        m2[2..4].copy_from_slice(&95u16.to_be_bytes());
        m2[4] = 2;
        m2[5..7].copy_from_slice(&0x0302u16.to_be_bytes());
        m2[9..17].copy_from_slice(&m[9..17]);
        let mic = crate::wpa::mic(&self.kck, &m2);
        m2[81..97].copy_from_slice(&mic);
        // after the 4-way handshake the AP only takes protected data: message 2 goes through the CCMP path
        let mut eth = [0u8; 14 + 99];
        eth[..6].copy_from_slice(&bssid);
        eth[6..12].copy_from_slice(&self.mac);
        eth[12..14].copy_from_slice(&[0x88, 0x8E]);
        eth[14..].copy_from_slice(&m2);
        self.tx_eth(&eth);
        crate::kprintln!("wifi: GTK rekey: {} group key id {}, message 2 sent (protected)", if fresh { "new" } else { "retried" }, gid);
    }

    fn wait_eapol(&mut self, ms: u32) -> Option<([u8; 512], usize)> {
        for _ in 0..ms {
            self.rx_poll(&mut |_, _, _| {});
            let e = unsafe { &mut *core::ptr::addr_of_mut!(EAPOL) };
            if e.1 >= 99 {
                let r = (e.0, e.1);
                e.1 = 0;
                return Some(r);
            }
            crate::wdt::pet();
            crate::wdt::delay_ms(1);
        }
        None
    }

    fn send_eapol(&mut self, ap: &[u8; 6], pdu: &[u8]) -> Result<u8, &'static str> {
        let mut f = [0u8; 600];
        f[0] = 0x08;
        f[1] = 0x01; // data, to-DS
        f[4..10].copy_from_slice(ap);
        f[10..16].copy_from_slice(&self.mac);
        f[16..22].copy_from_slice(ap);
        f[24..32].copy_from_slice(&[0xAA, 0xAA, 3, 0, 0, 0, 0x88, 0x8E]);
        f[32..32 + pdu.len()].copy_from_slice(pdu);
        self.tx_frame(2, &f[..32 + pdu.len()], 24, 0, 0x3018, 0, 8, 0)
    }

    /// §10 steps 1-8: join `ssid` (WPA2-PSK, CCMP) with the precomputed PMK.
    fn join(&mut self, ssid: &[u8], pmk: &[u8; 32], w: &mut dyn Write) -> Result<(), &'static str> {
        let need = self.need_privacy;
        let b = unsafe { (*core::ptr::addr_of!(BSS)).iter().flatten() }
            .filter(|b| &b.ssid[..b.ssid_len as usize] == ssid && need.is_none_or(|p| b.privacy == p))
            .max_by_key(|b| b.dbm);
        let b: Bss = match b {
            Some(b) => Bss { ssid: b.ssid, ssid_len: b.ssid_len, bssid: b.bssid, chan: b.chan, dbm: b.dbm, privacy: b.privacy, tsf: b.tsf, bint: b.bint,
                capab: b.capab, rates: b.rates, nrates: b.nrates, rsn: b.rsn, rsn_len: b.rsn_len, dtim: b.dtim },
            None => return Err("SSID not in the scan list (wifi scan first)"),
        };
        let _ = write!(w, "{} ch {} {} dBm capab {:#x} rsn {} B; ", core::str::from_utf8(ssid).unwrap_or("?"), b.chan, b.dbm, b.capab, b.rsn_len);
        if b.privacy && (b.rsn_len < 6 || b.rsn[2..6] != [0x00, 0x0F, 0xAC, 4]) {
            return Err("network needs TKIP or WPA1 (only WPA2-CCMP supported)");
        }
        // 2: un-associated RXON on the BSS, stations, TX power
        self.rxon_bss(&b, 0, 0x44)?;
        self.add_sta(15, &[0xFF; 6])?;
        self.lq(15)?;
        self.add_sta(0, &b.bssid)?;
        self.cmd(0x95, &[28, 0x40, 0x7F, 0])?;
        // 3: a beacon first (passive channels)
        unsafe {
            WATCH = Some(b.bssid);
            SEEN_TSF = None;
            (*core::ptr::addr_of_mut!(MGMT)).1 = 0;
            (*core::ptr::addr_of_mut!(EAPOL)).1 = 0;
        }
        for _ in 0..1500 {
            self.rx_poll(&mut |_, _, _| {});
            if unsafe { SEEN_TSF }.is_some() {
                break;
            }
            crate::wdt::pet();
            crate::wdt::delay_ms(1);
        }
        if unsafe { SEEN_TSF }.is_none() {
            return Err("no beacon from the BSSID after RXON");
        }
        // 4: open authentication
        let mut f = [0u8; 300];
        f[0] = 0xB0;
        f[4..10].copy_from_slice(&b.bssid);
        f[10..16].copy_from_slice(&self.mac);
        f[16..22].copy_from_slice(&b.bssid);
        f[26] = 1;
        let mut ok = false;
        for _ in 0..3 {
            let s = self.tx_frame(0, &f[..30], 24, 15, 0x3008, 0x420A, 8, 2)?;
            let _ = write!(w, "auth tx {:#x}; ", s);
            if let Some((m, n)) = self.wait_mgmt(0xB0, 300) {
                let fc = u16::from_le_bytes([m[0], m[1]]);
                if fc & 0xFC != 0xB0 {
                    return Err("deauth/disassoc during auth");
                }
                if n >= 30 && m[26] == 2 && m[28] == 0 && m[29] == 0 {
                    ok = true;
                    break;
                }
                let _ = write!(w, "auth status {}; ", u16::from_le_bytes([m[28], m[29]]));
            }
        }
        if !ok {
            return Err("authentication failed");
        }
        // 5: association request
        let mut f = [0u8; 300];
        f[4..10].copy_from_slice(&b.bssid);
        f[10..16].copy_from_slice(&self.mac);
        f[16..22].copy_from_slice(&b.bssid);
        let capab: u16 = 0x0001 | if b.privacy { 0x10 } else { 0 } | 0x0400 | 0x0020;
        f[24..26].copy_from_slice(&capab.to_le_bytes());
        f[26..28].copy_from_slice(&10u16.to_le_bytes());
        let mut i = 28;
        f[i] = 0;
        f[i + 1] = ssid.len() as u8;
        f[i + 2..i + 2 + ssid.len()].copy_from_slice(ssid);
        i += 2 + ssid.len();
        let rates: heapless::Vec<u8, 16> = b.rates[..b.nrates as usize].iter().map(|r| r & 0x7F).collect();
        let n1 = rates.len().min(8);
        f[i] = 1;
        f[i + 1] = n1 as u8;
        f[i + 2..i + 2 + n1].copy_from_slice(&rates[..n1]);
        i += 2 + n1;
        if rates.len() > 8 {
            let n2 = rates.len() - 8;
            f[i] = 50;
            f[i + 1] = n2 as u8;
            f[i + 2..i + 2 + n2].copy_from_slice(&rates[8..]);
            i += 2 + n2;
        }
        let rsn = our_rsn(&b);
        if b.privacy {
            f[i..i + 22].copy_from_slice(&rsn);
            i += 22;
        }
        let s = self.tx_frame(0, &f[..i], 24, 15, 0x3008, 0x420A, 8, 3)?;
        let _ = write!(w, "assoc tx {:#x}; ", s);
        let (m, n) = self.wait_mgmt(0x10, 500).ok_or("no association response")?;
        let fc = u16::from_le_bytes([m[0], m[1]]);
        if fc & 0xFC != 0x10 || n < 30 {
            return Err("deauth/disassoc instead of association response");
        }
        let status = u16::from_le_bytes([m[26], m[27]]);
        if status != 0 {
            let _ = write!(w, "assoc status {}; ", status);
            return Err("association refused");
        }
        let aid = u16::from_le_bytes([m[28], m[29]]) & 0x07FF;
        let _ = write!(w, "associated, AID {}; ", aid);
        // 6: associated configuration
        self.rxon_bss(&b, aid, 0x04)?;
        self.add_sta(15, &[0xFF; 6])?;
        self.lq(15)?;
        self.add_sta(0, &b.bssid)?;
        self.lq(0)?;
        let tsf = unsafe { SEEN_TSF }.unwrap_or(b.tsf);
        let bint = if b.bint == 0 { 200 } else { b.bint };
        let itm = bint as u64 * 1024;
        let mut tm = [0u8; 20];
        tm[0..8].copy_from_slice(&tsf.to_le_bytes());
        tm[8..10].copy_from_slice(&bint.to_le_bytes());
        tm[12..16].copy_from_slice(&((itm - tsf % itm) as u32).to_le_bytes());
        tm[16..18].copy_from_slice(&10u16.to_le_bytes());
        tm[18] = b.dtim;
        self.cmd(0x14, &tm)?;
        let mut qos = [0u8; 36];
        for a in 0..4 {
            qos[4 + 8 * a..12 + 8 * a].copy_from_slice(&[0x0F, 0, 0xFF, 0x03, 2, 0, 0, 0]);
        }
        self.cmd(0x13, &qos)?;
        self.rxon_bss(&b, aid, 0x24)?;
        self.cmd(0x95, &[28, 0x40, 0x7F, 0])?;
        if !b.privacy {
            self.bssid = b.bssid;
            self.open = true;
            self.joined = true;
            let _ = write!(w, "open network: joined");
            return Ok(());
        }
        self.open = false;
        // 7: the 4-way handshake
        let (m1, n1) = self.wait_eapol(3000).ok_or("no EAPOL message 1")?;
        let ki = u16::from_be_bytes([m1[5], m1[6]]);
        if m1[1] != 3 || m1[4] != 2 || ki & 0x0080 == 0 || ki & 0x0100 != 0 {
            let _ = write!(w, "m1 key_info {:#x}; ", ki);
            return Err("message 1 unexpected");
        }
        let mut anonce = [0u8; 32];
        anonce.copy_from_slice(&m1[17..49]);
        let mut snonce = [0u8; 32];
        crate::rng::fill(&mut snonce).map_err(|_| "no entropy for the SNonce")?;
        let ptk = crate::wpa::ptk(pmk, &b.bssid, &self.mac, &anonce, &snonce);
        let mut m2 = [0u8; 121];
        m2[0] = m1[0];
        m2[1] = 3;
        m2[2..4].copy_from_slice(&117u16.to_be_bytes());
        m2[4] = 2;
        m2[5..7].copy_from_slice(&0x010Au16.to_be_bytes());
        m2[9..17].copy_from_slice(&m1[9..17]);
        m2[17..49].copy_from_slice(&snonce);
        m2[97..99].copy_from_slice(&22u16.to_be_bytes());
        m2[99..121].copy_from_slice(&rsn);
        let mic = crate::wpa::mic(&ptk[..16], &m2);
        m2[81..97].copy_from_slice(&mic);
        let s = self.send_eapol(&b.bssid, &m2)?;
        let _ = write!(w, "m1 ({} B) m2 tx {:#x}; ", n1, s);
        let (m3, n3) = self.wait_eapol(3000).ok_or("no EAPOL message 3 (wrong passphrase?)")?;
        let ki3 = u16::from_be_bytes([m3[5], m3[6]]);
        if ki3 & 0x0040 == 0 || m3[17..49] != anonce || m3[9..17] <= m1[9..17] {
            let _ = write!(w, "m3 key_info {:#x}; ", ki3);
            // the AP re-sending message 1 (no MIC bit) means it rejected our MIC: the passphrase
            return Err(if ki3 & 0x0100 == 0 { "wrong passphrase" } else { "message 3 unexpected" });
        }
        let kdl = u16::from_be_bytes([m3[97], m3[98]]) as usize;
        if 99 + kdl > n3 {
            return Err("message 3 truncated");
        }
        let mut chk = [0u8; 512];
        chk[..99 + kdl].copy_from_slice(&m3[..99 + kdl]);
        chk[81..97].fill(0);
        if crate::wpa::mic(&ptk[..16], &chk[..99 + kdl]) != m3[81..97] {
            return Err("message 3 MIC wrong");
        }
        let mut kek = [0u8; 16];
        kek.copy_from_slice(&ptk[16..32]);
        let mut kd = [0u8; 256];
        if kdl < 24 || kdl > 256 + 8 || !crate::wpa::aes_unwrap(&kek, &m3[99..99 + kdl], &mut kd) {
            return Err("GTK unwrap failed");
        }
        let (gtk, gid) = parse_gtk(&kd, kdl - 8).ok_or("no GTK in message 3")?;
        let mut m4 = [0u8; 99];
        m4[0] = m3[0];
        m4[1] = 3;
        m4[2..4].copy_from_slice(&95u16.to_be_bytes());
        m4[4] = 2;
        m4[5..7].copy_from_slice(&0x030Au16.to_be_bytes());
        m4[9..17].copy_from_slice(&m3[9..17]);
        let mic = crate::wpa::mic(&ptk[..16], &m4);
        m4[81..97].copy_from_slice(&mic);
        let s = self.send_eapol(&b.bssid, &m4)?;
        let _ = write!(w, "m3 ok, GTK id {}; m4 tx {:#x}; ", gid, s);
        // 8: keys on station 0
        self.install_key(&b.bssid, 0x000A, 0, &ptk[32..48])?;
        self.install_key(&b.bssid, 0x400A | (gid as u16) << 8, 1, &gtk)?;
        self.gtk = gtk;
        self.tk.copy_from_slice(&ptk[32..48]);
        self.kck.copy_from_slice(&ptk[..16]);
        self.kek.copy_from_slice(&ptk[16..32]);
        self.rc.copy_from_slice(&m3[9..17]);
        self.ssid.0[..ssid.len().min(32)].copy_from_slice(&ssid[..ssid.len().min(32)]);
        self.ssid.1 = ssid.len().min(32) as u8;
        self.pmk = *pmk;
        self.lost = 0;
        self.pn = 0;
        let mut rsc = [0u8; 8];
        rsc[..6].copy_from_slice(&m3[65..71]);
        unsafe { RX_PN = [0, u64::from_le_bytes(rsc)] };
        self.bssid = b.bssid;
        self.joined = true;
        let _ = write!(w, "WPA2 keys installed: joined");
        Ok(())
    }
}

pub fn join_cmd<'a>(w: &mut dyn Write, it: &mut impl Iterator<Item = &'a str>) {
    let (Some(ssid), Some(hex)) = (it.next(), it.next()) else {
        let _ = write!(w, "err wifi join <ssid> <pmk: 64 hex>");
        return;
    };
    let mut pmk = [0u8; 32];
    if hex.len() != 64 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        let _ = write!(w, "err pmk must be 64 hex digits");
        return;
    }
    for i in 0..32 {
        pmk[i] = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap_or(0);
    }
    match st() {
        Some(s) if s.up && !s.scanning => {
            if let Err(e) = s.join(ssid.as_bytes(), &pmk, w) {
                let _ = write!(w, " err {}", e);
            }
        }
        _ => {
            let _ = write!(w, "err not up, or a scan is running");
        }
    }
}

// ======================= IP over WiFi: a smoltcp Ethernet device (docs/iwl-dvm-associate.md §7-8) =======================

/// WiFi data-path counters (bmctl wifi stats): data in, protected ok, decrypt refused (+ last status), to smoltcp,
/// from smoltcp, last REPLY_TX status of a data frame.
pub static mut WSTAT: [u32; 7] = [0; 7];

static mut ETH_RX: [([u8; 1600], usize); 16] = [([0; 1600], 0); 16];
static mut ETH_HEAD: usize = 0;
static mut ETH_TAIL: usize = 0;

fn eth_push(dst: &[u8], src: &[u8], et_payload: &[u8]) {
    unsafe {
        let n = 12 + et_payload.len();
        if n > 1600 || (ETH_TAIL + 1) % 16 == ETH_HEAD {
            return; // full: drop
        }
        let e = &mut (*core::ptr::addr_of_mut!(ETH_RX))[ETH_TAIL];
        e.0[..6].copy_from_slice(dst);
        e.0[6..12].copy_from_slice(src);
        e.0[12..n].copy_from_slice(et_payload);
        e.1 = n;
        ETH_TAIL = (ETH_TAIL + 1) % 16;
    }
}

impl St {
    /// An Ethernet frame from smoltcp, out as a CCMP-protected 802.11 data frame to the AP (§8).
    fn tx_eth(&mut self, eth: &[u8]) {
        if !self.joined || eth.len() < 14 || eth.len() > 1514 {
            return;
        }
        let mut f = [0u8; 1600];
        f[0] = 0x08;
        f[1] = 0x41; // data, to-DS, protected
        f[4..10].copy_from_slice(&self.bssid);
        f[10..16].copy_from_slice(&self.mac);
        f[16..22].copy_from_slice(&eth[0..6]);
        if self.open {
            f[1] = 0x01;
            f[24..30].copy_from_slice(&[0xAA, 0xAA, 3, 0, 0, 0]);
            let pl = &eth[12..];
            f[30..30 + pl.len()].copy_from_slice(pl);
            let _ = self.tx_frame(2, &f[..30 + pl.len()], 24, 0, 0x2018, 0, 8, 0);
            return;
        }
        self.pn += 1;
        let pn = self.pn.to_le_bytes();
        f[24..32].copy_from_slice(&[pn[0], pn[1], 0, 0x20, pn[2], pn[3], pn[4], pn[5]]);
        f[32..38].copy_from_slice(&[0xAA, 0xAA, 3, 0, 0, 0]);
        let pl = &eth[12..];
        f[38..38 + pl.len()].copy_from_slice(pl);
        let tk = self.tk;
        let r = self.tx_frame_k(2, &f[..38 + pl.len()], 24, 0, 0x2018, 0, 8, 0, Some(tk));
        unsafe {
            WSTAT[5] += 1;
            WSTAT[6] = match r { Ok(s) => s as u32, Err(_) => 0xFFFF };
        }
    }
}

pub struct WifiDev;
pub struct WRx(usize);
pub struct WTx;

impl smoltcp::phy::RxToken for WRx {
    fn consume<R, F: FnOnce(&[u8]) -> R>(self, f: F) -> R {
        let e = unsafe { &(*core::ptr::addr_of!(ETH_RX))[self.0] };
        let r = f(&e.0[..e.1]);
        unsafe { ETH_HEAD = (ETH_HEAD + 1) % 16 };
        r
    }
}

impl smoltcp::phy::TxToken for WTx {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        let mut b = [0u8; 1514];
        let r = f(&mut b[..len.min(1514)]);
        if let Some(s) = st() {
            s.tx_eth(&b[..len.min(1514)]);
        }
        r
    }
}

impl crate::net::Link for WifiDev {}

impl smoltcp::phy::Device for WifiDev {
    type RxToken<'a> = WRx;
    type TxToken<'a> = WTx;
    fn receive(&mut self, _t: smoltcp::time::Instant) -> Option<(WRx, WTx)> {
        if unsafe { ETH_HEAD == ETH_TAIL } {
            if let Some(s) = st().filter(|s| s.up) {
                s.rx_poll(&mut |_, _, _| {}); // drain the radio: the SSH loop polls only this device
                link_check(s);
            }
        }
        unsafe { (ETH_HEAD != ETH_TAIL).then_some((WRx(ETH_HEAD), WTx)) }
    }
    fn transmit(&mut self, _t: smoltcp::time::Instant) -> Option<WTx> {
        Some(WTx)
    }
    fn capabilities(&self) -> smoltcp::phy::DeviceCapabilities {
        let mut c = smoltcp::phy::DeviceCapabilities::default();
        c.medium = smoltcp::phy::Medium::Ethernet;
        c.max_transmission_unit = 1514;
        c.max_burst_size = Some(1);
        c
    }
}

use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet, SocketStorage};
use smoltcp::socket::dhcpv4;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpCidr};

static mut W_SOCKS: [SocketStorage<'static>; 4] = [SocketStorage::EMPTY; 4];
static mut W_UDP: Option<SocketHandle> = None;
static mut W_DNS: Option<smoltcp::wire::Ipv4Address> = None;
static mut W_DNSQ: [Option<smoltcp::socket::dns::DnsQuery>; 1] = [None];
static mut W_UDP_RXM: [smoltcp::socket::udp::PacketMetadata; 16] = [smoltcp::socket::udp::PacketMetadata::EMPTY; 16];
static mut W_UDP_TXM: [smoltcp::socket::udp::PacketMetadata; 16] = [smoltcp::socket::udp::PacketMetadata::EMPTY; 16];
static mut W_UDP_RX: [u8; 16384] = [0; 16384];
static mut W_UDP_TX: [u8; 16384] = [0; 16384];
/// Last CCMP packet number received: [pairwise, group].
static mut RX_PN: [u64; 2] = [0; 2];
static mut W_NET: Option<(Interface, SocketSet<'static>, SocketHandle)> = None;
static mut W_TCP: Option<SocketHandle> = None;
static mut W_TCP_RX: [u8; 16384] = [0; 16384];
static mut W_TCP_TX: [u8; 16384] = [0; 16384];

/// WiFi joined with an address: SSH sessions go out over it.
pub fn online() -> bool {
    unsafe { W_IP.is_some() && (*core::ptr::addr_of!(W_NET)).is_some() }
}

/// Run `f` with the WiFi interface, its sockets and its TCP socket (for an SSH session).
pub fn with_net<R>(f: impl FnOnce(&mut Interface, &mut WifiDev, &mut SocketSet<'static>, SocketHandle) -> R) -> Option<R> {
    let (iface, socks, _) = unsafe { (*core::ptr::addr_of_mut!(W_NET)).as_mut()? };
    let th = unsafe { W_TCP? };
    Some(f(iface, &mut WifiDev, socks, th))
}
static mut W_IP: Option<smoltcp::wire::Ipv4Cidr> = None;

fn now() -> smoltcp::time::Instant {
    smoltcp::time::Instant::from_millis(crate::time::now_ms())
}

/// Poll the WiFi IP stack (DHCP; smoltcp answers ARP and ping by itself).
pub fn net_poll() {
    let Some((iface, socks, dh)) = (unsafe { (*core::ptr::addr_of_mut!(W_NET)).as_mut() }) else { return };
    let mut dev = WifiDev;
    iface.poll(now(), &mut dev, socks);
    if let Some(dhcpv4::Event::Configured(c)) = socks.get_mut::<dhcpv4::Socket>(*dh).poll() {
        iface.update_ip_addrs(|a| {
            a.clear();
            let _ = a.push(IpCidr::Ipv4(c.address));
        });
        if let Some(gw) = c.router {
            let _ = iface.routes_mut().add_default_ipv4_route(gw);
        }
        unsafe {
            W_IP = Some(c.address);
            W_DNS = c.dns_servers.first().copied().or(c.router);
        }
        crate::kprintln!("wifi: DHCP {} gw {:?}", c.address, c.router);
    }
}

pub fn dhcp_cmd(w: &mut dyn Write) {
    let Some(s) = st().filter(|s| s.joined) else {
        let _ = write!(w, "err not joined (wifi join)");
        return;
    };
    if unsafe { (*core::ptr::addr_of!(W_NET)).is_none() } {
        let mut seed = [0u8; 8];
        let _ = crate::rng::fill(&mut seed);
        let mut cfg = Config::new(HardwareAddress::Ethernet(EthernetAddress(s.mac)));
        cfg.random_seed = u64::from_le_bytes(seed);
        let mut dev = WifiDev;
        let iface = Interface::new(cfg, &mut dev, now());
        // a rejoin dropped the old set, but its sockets are still in the static storage: empty it, or add() panics
        unsafe { *core::ptr::addr_of_mut!(W_SOCKS) = [const { SocketStorage::EMPTY }; 4] };
        let mut socks = SocketSet::new(unsafe { &mut *core::ptr::addr_of_mut!(W_SOCKS) }[..].as_mut());
        let dh = socks.add(dhcpv4::Socket::new());
        let th = socks.add(smoltcp::socket::tcp::Socket::new(
            smoltcp::socket::tcp::SocketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(W_TCP_RX) }[..].as_mut()),
            smoltcp::socket::tcp::SocketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(W_TCP_TX) }[..].as_mut()),
        ));
        let mut u = smoltcp::socket::udp::Socket::new(
            smoltcp::socket::udp::PacketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(W_UDP_RXM) }[..].as_mut(), unsafe { &mut *core::ptr::addr_of_mut!(W_UDP_RX) }[..].as_mut()),
            smoltcp::socket::udp::PacketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(W_UDP_TXM) }[..].as_mut(), unsafe { &mut *core::ptr::addr_of_mut!(W_UDP_TX) }[..].as_mut()),
        );
        let _ = u.bind(51820); // the WireGuard client's source port (wg.rs)
        let uh = socks.add(u);
        unsafe {
            *core::ptr::addr_of_mut!(W_NET) = Some((iface, socks, dh));
            W_TCP = Some(th);
            W_UDP = Some(uh);
        }
    }
    for _ in 0..8000 {
        poll();
        if unsafe { W_IP }.is_some() {
            break;
        }
        crate::wdt::pet();
        crate::wdt::delay_ms(1);
    }
    let _ = match unsafe { W_IP } {
        Some(ip) => write!(w, "WiFi IP {}", ip),
        None => write!(w, "no DHCP lease in 8 s (still trying in the background)"),
    };
}

// ======================= API for the on-device WiFi screen (ssh::wifi_screen) =======================

struct Sink;
impl core::fmt::Write for Sink {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        crate::kprint!("{}", s);
        Ok(())
    }
}

pub fn is_up() -> bool {
    st().is_some_and(|s| s.up)
}

pub fn bring_up() -> Result<(), &'static str> {
    if is_up() {
        return Ok(());
    }
    up(&mut Sink)
}

pub fn scan_start() -> Result<(), &'static str> {
    unsafe { *core::ptr::addr_of_mut!(BSS) = [const { None }; NBSS] };
    if st().is_some_and(|s| s.bssid != [0; 6]) {
        // after a join (or a failed one) a scan never completes and SCAN_ABORT crashes the firmware: restart the radio
        unsafe { W_IP = None };
        up(&mut Sink)?;
    }
    st().filter(|s| s.up).ok_or("radio not up")?.scan()
}

pub fn scanning() -> bool {
    st().is_some_and(|s| s.scanning)
}

/// Networks heard, strongest first, one entry per SSID (hidden ones left out): (ssid, channel, dBm, secured).
pub fn networks(out: &mut heapless::Vec<(heapless::String<32>, u8, i8, bool), 16>) {
    out.clear();
    let l = unsafe { &*core::ptr::addr_of!(BSS) };
    let mut v: heapless::Vec<&Bss, NBSS> = l.iter().flatten().filter(|b| b.ssid[..b.ssid_len as usize].iter().any(|&c| c > b' ')).collect();
    v.sort_unstable_by_key(|b| -(b.dbm as i32));
    for b in v {
        let name = core::str::from_utf8(&b.ssid[..b.ssid_len as usize]).unwrap_or("?");
        if out.iter().any(|e| e.0.as_str() == name) {
            continue;
        }
        let mut s = heapless::String::new();
        let _ = s.push_str(name);
        if out.push((s, b.chan, b.dbm, b.privacy)).is_err() {
            break;
        }
    }
}

pub fn join_with(ssid: &str, pmk: &[u8; 32]) -> Result<(), &'static str> {
    let t0 = crate::time::now_ms();
    while scanning() && crate::time::now_ms() - t0 < 6000 {
        poll();
    }
    let s = st().filter(|s| s.up && !s.scanning).ok_or("radio not ready (scan still running)")?;
    s.joined = false;
    unsafe {
        W_IP = None;
        *core::ptr::addr_of_mut!(W_NET) = None; // fresh DHCP, address and routes on the new network
        W_TCP = None;
        W_UDP = None;
    }
    s.join(ssid.as_bytes(), pmk, &mut Sink)
}

/// DHCP over WiFi, waiting up to `ms`.
pub fn dhcp_wait(ms: u32) -> Option<smoltcp::wire::Ipv4Cidr> {
    let mut sink = Sink;
    let _ = ms;
    dhcp_cmd(&mut sink);
    unsafe { W_IP }
}

/// (SSID, address) once WiFi has a lease.
pub fn address() -> Option<(heapless::String<32>, smoltcp::wire::Ipv4Cidr)> {
    let ip = unsafe { W_IP }?;
    let s = st()?;
    let b = unsafe { (*core::ptr::addr_of!(BSS)).iter().flatten().find(|b| b.bssid == s.bssid) }?;
    let mut n = heapless::String::new();
    let _ = n.push_str(core::str::from_utf8(&b.ssid[..b.ssid_len as usize]).unwrap_or("?"));
    Some((n, ip))
}

/// The WiFi UDP socket (WireGuard's outer transport).
pub fn udp_send(to: smoltcp::wire::IpEndpoint, d: &[u8]) -> bool {
    let (Some((_, socks, _)), Some(h)) = (unsafe { (*core::ptr::addr_of_mut!(W_NET)).as_mut() }, unsafe { W_UDP }) else {
        crate::kprintln!("wifi: udp send without a WiFi socket");
        return false;
    };
    match socks.get_mut::<smoltcp::socket::udp::Socket>(h).send_slice(d, to) {
        Ok(()) => true,
        Err(e) => {
            crate::kprintln!("wifi: udp send to {} failed: {:?}", to, e);
            false
        }
    }
}

pub fn udp_recv(buf: &mut [u8]) -> Option<usize> {
    let (Some((_, socks, _)), Some(h)) = (unsafe { (*core::ptr::addr_of_mut!(W_NET)).as_mut() }, unsafe { W_UDP }) else { return None };
    socks.get_mut::<smoltcp::socket::udp::Socket>(h).recv_slice(buf).ok().map(|(n, _)| n)
}

/// A-record lookup over WiFi through the DHCP-given DNS server, waiting up to `ms`.
pub fn resolve(name: &str, ms: i64) -> Option<smoltcp::wire::Ipv4Address> {
    use smoltcp::socket::dns;
    let srv = unsafe { W_DNS }?;
    let (iface, socks, _) = unsafe { (*core::ptr::addr_of_mut!(W_NET)).as_mut() }?;
    let dh = socks.add(dns::Socket::new(&[smoltcp::wire::IpAddress::Ipv4(srv)], unsafe { &mut *core::ptr::addr_of_mut!(W_DNSQ) }[..].as_mut()));
    let q = socks.get_mut::<dns::Socket>(dh).start_query(iface.context(), name, smoltcp::wire::DnsQueryType::A).ok();
    let t0 = crate::time::now_ms();
    let mut out = None;
    while let Some(q) = q {
        poll();
        let Some((_, socks, _)) = (unsafe { (*core::ptr::addr_of_mut!(W_NET)).as_mut() }) else { break };
        match socks.get_mut::<dns::Socket>(dh).get_query_result(q) {
            Ok(a) => {
                out = a.iter().find_map(|x| match x { smoltcp::wire::IpAddress::Ipv4(v) => Some(*v), #[allow(unreachable_patterns)] _ => None });
                break;
            }
            Err(dns::GetQueryResultError::Pending) if crate::time::now_ms() - t0 < ms => crate::wdt::pet(),
            Err(dns::GetQueryResultError::Pending) => {
                socks.get_mut::<dns::Socket>(dh).cancel_query(q); // timed out: the slot is still ours
                break;
            }
            Err(_) => break, // Failed (e.g. NXDOMAIN) already freed the slot; cancelling it would panic

        }
    }
    if let Some((_, socks, _)) = unsafe { (*core::ptr::addr_of_mut!(W_NET)).as_mut() } {
        socks.remove(dh);
    }
    unsafe { W_DNSQ = [None] };
    out
}

/// Before a chain-load: the radio's DMA (RX ring, firmware) would otherwise keep writing into memory the new kernel
/// owns. Stop master, software reset, bus mastering off. Ok(()) if there is no card or it never bus-mastered.
pub fn quiesce() -> Result<(), &'static str> {
    let Some(bdf) = crate::pci::find(0x8086, 0x0896) else { return Ok(()) };
    let cmd = crate::pci::read32(bdf, 4);
    if cmd & 0x4 == 0 {
        return Ok(());
    }
    let mut stopped = true;
    if let Some(s) = st() {
        s.d.wr(CSR_RESET, 0x200); // STOP_MASTER
        stopped = (0..100).any(|_| {
            crate::wdt::delay_ms(1);
            s.d.rd(CSR_RESET) & 0x100 != 0 // MASTER_DISABLED
        });
        s.d.wr(CSR_RESET, 0x80); // SW_RESET
        crate::wdt::delay_ms(5);
    }
    crate::pci::write32(bdf, 4, cmd & 0xFFFF & !0x4);
    unsafe {
        *core::ptr::addr_of_mut!(ST) = None;
        *core::ptr::addr_of_mut!(W_NET) = None;
        W_IP = None;
    }
    crate::kprintln!("iwl: quiesced (master stop {})", if stopped { "ok" } else { "timed out; bus master cleared anyway" });
    Ok(())
}
