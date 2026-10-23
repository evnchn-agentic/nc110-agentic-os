//! Intel High Definition Audio: internal-mic capture for push-to-talk. Finds the controller (PCI class 04:03), resets it,
//! talks to the codec over the Immediate Command interface (ICOI/IRII/ICIS: no CORB/RIRB DMA). `hdadump` logs every
//! widget (type, pin default configuration). Capture path (NC110: Realtek ALC269, proof/nc110-hda-discovery.log):
//! internal mic pin 0x19 -> selector 0x22 (input 1) -> ADC 0x09, 48 kHz stereo 16-bit into a 256 KiB DMA ring on input
//! stream 0, decimated here to 16 kHz mono 16-bit into a RAM clip (max 30 s). The clip leaves the box only over bmctl
//! (`rec`, `aud`: authenticated and encrypted). On by default in the attended agent OS (`nohda` turns it off).
use crate::{kprintln, nic, pci, time, wdt};
use core::ptr::{read_volatile, write_volatile};

const GCTL: u64 = 0x08;
const STATESTS: u64 = 0x0E;
const ICOI: u64 = 0x60;
const IRII: u64 = 0x64;
const ICIS: u64 = 0x68;

#[derive(Clone, Copy)]
struct Hda(u64);

impl Hda {
    fn r8(&self, o: u64) -> u8 {
        unsafe { read_volatile((self.0 + o) as *const u8) }
    }
    fn w8(&self, o: u64, v: u8) {
        unsafe { write_volatile((self.0 + o) as *mut u8, v) }
    }
    fn r16(&self, o: u64) -> u16 {
        unsafe { read_volatile((self.0 + o) as *const u16) }
    }
    fn r32(&self, o: u64) -> u32 {
        unsafe { read_volatile((self.0 + o) as *const u32) }
    }
    fn w16(&self, o: u64, v: u16) {
        unsafe { write_volatile((self.0 + o) as *mut u16, v) }
    }
    fn w32(&self, o: u64, v: u32) {
        unsafe { write_volatile((self.0 + o) as *mut u32, v) }
    }

    /// One verb over the immediate interface: wait not busy, write, set ICB, wait for IRV. Bounded (~10 ms each).
    fn verb(&self, cad: u32, nid: u32, verb: u32) -> Option<u32> {
        for _ in 0..1000 {
            if self.r16(ICIS) & 1 == 0 {
                break;
            }
            wdt::delay_ms(0);
        }
        self.w16(ICIS, 2); // clear IRV (RW1C)
        self.w32(ICOI, cad << 28 | nid << 20 | verb);
        self.w16(ICIS, 1); // ICB: send
        for _ in 0..1000 {
            let s = self.r16(ICIS);
            if s & 1 == 0 && s & 2 != 0 {
                return Some(self.r32(IRII));
            }
            for _ in 0..100 {
                core::hint::spin_loop();
            }
        }
        None
    }

    fn param(&self, cad: u32, nid: u32, p: u32) -> Option<u32> {
        self.verb(cad, nid, 0xF00 << 8 | p)
    }
}

const TYPES: [&str; 8] = ["out", "in", "mix", "sel", "pin", "pwr", "vol", "beep"];
const DEVS: [&str; 16] = ["LineOut", "Speaker", "HPOut", "CD", "SPDIFOut", "DigOut", "ModemLine", "ModemHS", "LineIn", "AUX",
    "MicIn", "Telephony", "SPDIFIn", "DigIn", "rsvd", "Other"];
const CONN: [&str; 4] = ["jack", "none", "fixed", "both"];

pub fn probe(dump: bool) {
    let mut hit = None;
    pci::walk(&mut |b, _| {
        if pci::read32(b, 8) >> 8 == 0x04_0300 {
            hit = Some(b);
            return true;
        }
        false
    });
    let Some(b) = hit else {
        kprintln!("hda: no HD Audio controller (class 04:03)");
        return;
    };
    let Some((bar, _)) = pci::first_mem_bar(b) else {
        kprintln!("hda: controller has no memory BAR");
        return;
    };
    kprintln!("hda: {:#010x} at {} BAR {:#x}", pci::read32(b, 0), b, bar);
    if !nic::map_uc_4k(bar) {
        kprintln!("hda: no free UC page table");
        return;
    }
    pci::write32(b, 4, pci::read32(b, 4) & 0xFFFF | 0x2); // memory space on (status half written 0); no bus mastering yet
    let h = Hda(bar);
    kprintln!("hda: GCAP {:#06x} VMAJ.VMIN {}.{}", h.r16(0), unsafe { read_volatile((bar + 3) as *const u8) },
        unsafe { read_volatile((bar + 2) as *const u8) });
    // Controller reset: CRST 0, wait, CRST 1, wait, then give codecs 10 ms to report in STATESTS.
    h.w32(GCTL, h.r32(GCTL) & !1);
    for _ in 0..100 {
        if h.r32(GCTL) & 1 == 0 {
            break;
        }
        wdt::delay_ms(1);
    }
    h.w32(GCTL, h.r32(GCTL) | 1);
    for _ in 0..100 {
        if h.r32(GCTL) & 1 != 0 {
            break;
        }
        wdt::delay_ms(1);
    }
    wdt::delay_ms(10);
    let codecs = h.r16(STATESTS);
    kprintln!("hda: GCTL {:#x} STATESTS {:#06x}", h.r32(GCTL), codecs);
    for cad in 0..15u32 {
        if codecs & (1 << cad) == 0 {
            continue;
        }
        let vid = h.param(cad, 0, 0x00);
        let sub = h.param(cad, 0, 0x04);
        kprintln!("hda: codec {} vendor {:x?} rev {:x?} nodes {:x?}", cad, vid, h.param(cad, 0, 0x02), sub);
        let Some(sub) = sub else { continue };
        if !dump {
            continue;
        }
        for fg in (sub >> 16) & 0xFF..((sub >> 16) & 0xFF) + (sub & 0xFF) {
            let ty = h.param(cad, fg, 0x05).unwrap_or(0) & 0xFF;
            let ws = h.param(cad, fg, 0x04).unwrap_or(0);
            kprintln!("hda: codec {} fg {:#x} type {} (1 = audio) widgets {:#x}..+{}", cad, fg, ty, (ws >> 16) & 0xFF, ws & 0xFF);
            if ty != 1 {
                continue;
            }
            for nid in (ws >> 16) & 0xFF..((ws >> 16) & 0xFF) + (ws & 0xFF) {
                wdt::pet();
                let caps = h.param(cad, nid, 0x09).unwrap_or(0);
                let wt = ((caps >> 20) & 0xF) as usize;
                let cl = h.param(cad, nid, 0x0E).unwrap_or(0) & 0x7F;
                let mut conns = [0u8; 8];
                let n = (cl as usize).min(8);
                let mut i = 0;
                while i < n {
                    let e = h.verb(cad, nid, 0xF02 << 8 | i as u32).unwrap_or(0); // 4 short-form entries per response
                    for k in 0..4 {
                        if i + k < n {
                            conns[i + k] = (e >> (8 * k)) as u8;
                        }
                    }
                    i += 4;
                }
                if wt == 4 {
                    let cfg = h.verb(cad, nid, 0xF1C << 8).unwrap_or(0);
                    let pc = h.param(cad, nid, 0x0C).unwrap_or(0);
                    kprintln!("hda:  {:#04x} pin {:<9} {} loc {:#04x} caps {:#x}{}{} cfg {:#010x} conn {:x?}", nid,
                        DEVS[(cfg >> 20 & 0xF) as usize], CONN[(cfg >> 30) as usize], cfg >> 24 & 0x3F, pc,
                        if pc & (1 << 5) != 0 { " IN" } else { "" }, if pc & (1 << 4) != 0 { " OUT" } else { "" }, cfg, &conns[..n]);
                } else {
                    kprintln!("hda:  {:#04x} {:<4} caps {:#010x} conn {:x?}", nid, TYPES.get(wt).unwrap_or(&"?"), caps, &conns[..n]);
                }
            }
        }
    }    if codecs & 1 == 0 || h.param(0, 0, 0x00) != Some(0x10ec_0269) {
        kprintln!("hda: codec 0 is not the ALC269 this capture path was mapped for: no capture");
        return;
    }
    pci::write32(b, 4, pci::read32(b, 4) & 0xFFFF | 0x6); // memory space + bus mastering (stream DMA)
    // TCSEL (0x44): this BIOS leaves TC7, which maps stream DMA to a virtual channel nobody set up, so the stream runs
    // but LPIB never moves (measured on the NC110). TC0 as Linux's azx_init_pci does for ICH.
    pci::write32(b, 0x44, pci::read32(b, 0x44) & !7);
    setup_codec(&h);
    unsafe { DEV = Some(h) };
}

const AFG: u32 = 0x01;
const ADC: u32 = 0x09;
const SEL: u32 = 0x22;
const SEL_MIC: u32 = 1; // 0x22's connection list: [18, 19, 1a, 1b, 1d, b, 12]
const MIC: u32 = 0x19;
const SD: u64 = 0x80; // input stream descriptor 0 (GCAP ISS = 4)
const TAG: u8 = 1;
const FMT_48K_STEREO_16: u16 = 0x0011;
const RING: usize = 4 * 65536; // ~1.4 s at 48 kHz stereo 16-bit
const NBDL: usize = 4;
const CLIP_MAX: usize = 16_000 * 30; // samples

#[repr(C, align(128))]
struct Bdl([[u32; 4]; NBDL]);
#[repr(C, align(128))]
struct Ring([u8; RING]);
static mut DEV: Option<Hda> = None;
static mut BDL: Bdl = Bdl([[0; 4]; NBDL]);
static mut DMA: Ring = Ring([0; RING]);
static mut CLIP: [i16; CLIP_MAX] = [0; CLIP_MAX];
static mut CLIP_LEN: usize = 0;
static mut CLIP_ID: u32 = 0;
static mut PEAK: i16 = 0;
static mut REC_UNTIL: Option<i64> = None;
static mut LAST_POS: usize = 0;
static mut ACC: (i32, u32) = (0, 0); // decimation: sum of mono samples, count (3 -> one 16 kHz sample)
static mut LAST_POLL: i64 = 0;
static mut OVERRUN: bool = false;

fn setup_codec(h: &Hda) {
    let v = |nid: u32, verb: u32| h.verb(0, nid, verb);
    for nid in [AFG, ADC, SEL, MIC] {
        let _ = v(nid, 0x705 << 8); // power state D0
    }
    let _ = v(MIC, 0x707 << 8 | 0x24); // pin control: input enable, VREF 80 % (mic bias)
    let mic_amp = h.param(0, MIC, 0x0D).unwrap_or(0);
    let mic_gain = ((mic_amp >> 8) & 0x7F).min(2); // boost steps (typically 10 dB each): +20 dB
    let _ = v(MIC, 0x3 << 16 | 0x7000 | mic_gain);
    let _ = v(SEL, 0x701 << 8 | SEL_MIC);
    let sel_amp = h.param(0, SEL, 0x0D).unwrap_or(0);
    let _ = v(SEL, 0x3 << 16 | 0x7000 | SEL_MIC << 8 | (sel_amp & 0x7F));
    let adc_amp = h.param(0, ADC, 0x0D).unwrap_or(0);
    let _ = v(ADC, 0x3 << 16 | 0x7000 | (adc_amp & 0x7F)); // 0 dB (the caps' offset step), unmuted
    let _ = v(ADC, 0x2 << 16 | FMT_48K_STEREO_16 as u32);
    let _ = v(ADC, 0x706 << 8 | (TAG as u32) << 4);
    kprintln!("hda: readback adc fmt {:x?} stream {:x?} pwr {:x?} | sel conn {:x?} pwr {:x?} | mic pinctl {:x?} pwr {:x?} | afg pwr {:x?}",
        v(ADC, 0xA << 16), v(ADC, 0xF06 << 8), v(ADC, 0xF05 << 8), v(SEL, 0xF01 << 8), v(SEL, 0xF05 << 8),
        v(MIC, 0xF07 << 8), v(MIC, 0xF05 << 8), v(AFG, 0xF05 << 8));
    kprintln!("hda: capture path mic {:#x} (boost {} of caps {:#x}) -> sel {:#x}[{}] (caps {:#x}) -> adc {:#x} (caps {:#x}); \
        adc rates {:#x}", MIC, mic_gain, mic_amp, SEL, SEL_MIC, sel_amp, ADC, adc_amp, h.param(0, ADC, 0x0A).unwrap_or(0));
}

/// Start a recording of up to `ms` (capped at 30 s). Replaces the previous clip. Err = no capture hardware.
pub fn rec_start(ms: i64) -> Result<u32, &'static str> {
    let Some(h) = (unsafe { DEV }) else { return Err("no capture (cmdline hda, ALC269)") };
    unsafe {
        h.w8(SD, 0); // stop
        h.w8(SD, 1); // SRST
        for _ in 0..1000 {
            if h.r8(SD) & 1 != 0 {
                break;
            }
        }
        h.w8(SD, 0);
        for _ in 0..1000 {
            if h.r8(SD) & 1 == 0 {
                break;
            }
        }
        let base = core::ptr::addr_of!(DMA) as u64;
        let bdl = &mut *core::ptr::addr_of_mut!(BDL);
        for (i, e) in bdl.0.iter_mut().enumerate() {
            let a = base + (i * RING / NBDL) as u64;
            *e = [a as u32, (a >> 32) as u32, (RING / NBDL) as u32, 0];
        }
        let bdla = core::ptr::addr_of!(BDL) as u64;
        h.w8(SD + 3, 0x1C); // clear status
        h.w32(SD + 0x08, RING as u32); // CBL
        h.w16(SD + 0x0C, (NBDL - 1) as u16); // LVI
        h.w16(SD + 0x12, FMT_48K_STEREO_16);
        h.w32(SD + 0x18, bdla as u32);
        h.w32(SD + 0x1C, (bdla >> 32) as u32);
        h.w8(SD + 2, TAG << 4); // stream number
        CLIP_LEN = 0;
        PEAK = 0;
        LAST_POS = 0;
        ACC = (0, 0);
        CLIP_ID += 1;
        OVERRUN = false;
        LAST_POLL = time::now_ms();
        REC_UNTIL = Some(time::now_ms() + ms.clamp(100, 30_000));
        crate::console::corner(Some(b"REC")); // any recording, F12 or bmctl `rec`, is visible on the screen
        h.w8(SD, 0x02); // RUN
        let id = CLIP_ID;
        kprintln!("hda: recording clip {} for {} ms", id, ms.clamp(100, 30_000));
        Ok(id)
    }
}

/// F12: start a recording (up to 30 s) or stop the running one. Returns a status line for the screen.
pub fn ptt_toggle() -> &'static str {
    if unsafe { (*core::ptr::addr_of!(REC_UNTIL)).is_some() } {
        rec_stop();
        "transcribing"
    } else {
        match rec_start(30_000) {
            Ok(_) => "recording: speak, F12 to stop",
            Err(_) => "no microphone",
        }
    }
}

static mut TYPED: [u8; 1024] = [0; 1024];
static mut TYPED_LEN: usize = 0;

/// bmctl `type <text>`: the transcript to put in front of the user (typed into SSH, shown at home).
pub fn type_text(t: &[u8]) -> usize {
    unsafe {
        let buf = &mut *core::ptr::addr_of_mut!(TYPED);
        let n = t.len().min(buf.len() - TYPED_LEN);
        buf[TYPED_LEN..TYPED_LEN + n].copy_from_slice(&t[..n]);
        TYPED_LEN += n;
        n
    }
}

/// Move up to `out.len()` queued transcript bytes into `out`; returns how many.
pub fn take_typed(out: &mut [u8]) -> usize {
    unsafe {
        let buf = &mut *core::ptr::addr_of_mut!(TYPED);
        let n = out.len().min(TYPED_LEN);
        out[..n].copy_from_slice(&buf[..n]);
        buf.copy_within(n..TYPED_LEN, 0);
        TYPED_LEN -= n;
        n
    }
}

pub fn rec_stop() {
    unsafe {
        if let (Some(h), Some(_)) = (DEV, REC_UNTIL) {
            h.w8(SD, 0);
            REC_UNTIL = None;
            crate::console::corner(None);
            let (id, n, p) = (CLIP_ID, CLIP_LEN, PEAK);
            kprintln!("hda: clip {} done: {} samples ({} ms), peak {}", id, n, n / 16, p);
        }
    }
}

/// Drain the DMA ring into the clip (called every pass of the bmctl loop; the ring holds ~340 ms).
pub fn poll(now: i64) {
    unsafe {
        let (Some(h), Some(until)) = (DEV, REC_UNTIL) else { return };
        // The ring holds ~1.4 s; a pass that came back later than 1 s may have been lapped: drop the clip, say so.
        if now - LAST_POLL > 1000 {
            OVERRUN = true;
            CLIP_LEN = 0;
            rec_stop();
            kprintln!("hda: capture overrun ({} ms between polls): clip dropped", now - LAST_POLL);
            return;
        }
        LAST_POLL = now;
        let pos = (h.r32(SD + 0x04) as usize & !3) % RING; // LPIB, whole stereo frames
        let ring = &*core::ptr::addr_of!(DMA);
        let clip = &mut *core::ptr::addr_of_mut!(CLIP);
        while LAST_POS != pos && CLIP_LEN < CLIP_MAX {
            let f = &ring.0[LAST_POS..LAST_POS + 4];
            let l = i16::from_le_bytes([f[0], f[1]]) as i32;
            let r = i16::from_le_bytes([f[2], f[3]]) as i32;
            ACC.0 += (l + r) / 2;
            ACC.1 += 1;
            if ACC.1 == 3 {
                let s = (ACC.0 / 3) as i16;
                clip[CLIP_LEN] = s;
                CLIP_LEN += 1;
                PEAK = PEAK.max(s.saturating_abs());
                ACC = (0, 0);
            }
            LAST_POS = (LAST_POS + 4) % RING;
        }
        if now >= until || CLIP_LEN >= CLIP_MAX {
            rec_stop();
        }
    }
}

/// (clip id, bytes, recording, peak, overrun)
pub fn status() -> (u32, usize, bool, i16, bool) {
    unsafe { (CLIP_ID, CLIP_LEN * 2, (*core::ptr::addr_of!(REC_UNTIL)).is_some(), PEAK, OVERRUN) }
}

/// The clip as 16-bit little-endian PCM, 16 kHz mono.
pub fn clip_bytes() -> &'static [u8] {
    unsafe { core::slice::from_raw_parts(core::ptr::addr_of!(CLIP) as *const u8, CLIP_LEN * 2) }
}

/// Stream descriptor 0 registers for `aud`: (SD_CTL 24 bit, SD_STS, LPIB, GCTL).
pub fn dump() {
    if let Some(h) = unsafe { DEV } {
        let b = pci::b0(0x1b, 0);
        let w0 = h.r32(0x30);
        wdt::delay_ms(5);
        kprintln!("hda: pci 40 {:#010x} 44 {:#010x} 4c {:#010x} 50 {:#010x} | walclk {} -> {} (5 ms) | sd0 sts {:#x}",
            pci::read32(b, 0x40), pci::read32(b, 0x44), pci::read32(b, 0x4C), pci::read32(b, 0x50), w0, h.r32(0x30), h.r8(SD + 3));
        // No DMA ring contents here: the log is broadcast in clear by udplog, and the ring holds room audio.
        kprintln!("hda: regs SD0 ctl {:#x} lpib {} cbl {} lvi {} fifos {} fmt {:#x} bdpl {:#x} | ssync34 {:#x} ssync38 {:#x} intctl {:#x} intsts {:#x} | bdl0 {:x?} | pcicmd {:#x}",
            h.r32(SD), h.r32(SD + 4), h.r32(SD + 8), h.r16(SD + 0xC), h.r16(SD + 0x10), h.r16(SD + 0x12), h.r32(SD + 0x18),
            h.r32(0x34), h.r32(0x38), h.r32(0x20), h.r32(0x24), unsafe { (*core::ptr::addr_of!(BDL)).0[0] },
            pci::read32(pci::b0(0x1b, 0), 4));
    }
}

pub fn regs() -> (u32, u8, u32, u32) {
    match unsafe { DEV } {
        Some(h) => (h.r32(SD) & 0xFF_FFFF, h.r8(SD + 3), h.r32(SD + 4), h.r32(GCTL)),
        None => (0, 0, 0, 0),
    }
}

/// Before control leaves this kernel (chain-load): the capture DMA must not keep writing into memory the next kernel
/// is about to own (Codex review MUST-FIX 1). Stop the stream, wait for RUN to read back clear, reset it, and turn the
/// controller's bus mastering off. Err = the stream did not stop: the caller must not jump.
pub fn quiesce() -> Result<(), &'static str> {
    unsafe {
        let Some(h) = DEV else { return Ok(()) };
        h.w8(SD, 0);
        let mut stopped = false;
        for _ in 0..1000 {
            if h.r8(SD) & 0x02 == 0 {
                stopped = true;
                break;
            }
            wdt::delay_ms(1);
        }
        h.w8(SD, 1); // stream reset: the descriptor list is forgotten
        REC_UNTIL = None;
        DEV = None;
        if let Some(b) = find() {
            pci::write32(b, 4, pci::read32(b, 4) & 0xFFFF & !0x4);
        }
        if stopped { Ok(()) } else { Err("HD Audio stream did not stop") }
    }
}

fn find() -> Option<pci::Bdf> {
    let mut hit = None;
    pci::walk(&mut |b, _| {
        if pci::read32(b, 8) >> 8 == 0x04_0300 {
            hit = Some(b);
            return true;
        }
        false
    });
    hit
}
