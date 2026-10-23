//! Entropy for Sunset (getrandom custom backend). M2.5: CPU-jitter source first, RDRAND only as an extra.
//!
//! The NC110's Atom N570 has NO RDRAND. The noise source races execution against the TSC: count strided
//! walk steps (2 MiB buffer) completed while the TSC advances by a fixed window (see `raw_delta`).
//! Interrupts are off, so all jitter comes from the microarchitecture (cache/DRAM/bus timing) or, in QEMU,
//! from the HOST: TCG's TSC moves in 1000-tick (1 us host clock) steps and the count reflects host speed
//! noise, so QEMU numbers say nothing about the N570. In-order Atoms are the known weak case
//! for jitter sources; the claim below must be re-measured on the real board (cmdline `rng=dump`).
//!
//! H_CLAIM = 0.1 bit per 8-bit raw sample (low byte of each count). Chosen BELOW the worst QEMU estimate
//! (lag-1 0.198, tools/entropy-est.py on proof/m25-rngdump-serial.log), not derived from hardware.
//! SP 800-90B-style health tests, false-positive rate alpha = 2^-30, cutoffs from H_CLAIM:
//!   Repetition Count Test (4.4.1): cutoff 1 + ceil(30 / 0.1) = 301.
//!   Adaptive Proportion Test (4.4.2): W = 512, cutoff 507 = smallest C with P(1 + Bin(511, 2^-0.1) >= C)
//!   <= 2^-30 (computed: tools/entropy-est.py --cutoffs 0.1). At this H the APT is weak by construction.
//! Startup: 4096 samples must pass both tests AND both in-kernel estimates (MCV, SP 800-90B 6.3.1, and a
//! lag-1 conditional most-common-value bound that catches serial correlation) must be >= H_CLAIM, else refuse.
//! Continuous: every sample is tested; one failure latches FAILED and every later request errors.
//!
//! Conditioning: SHA-256 (the pinned soft `sha2`) over the full 64-bit counts. Credit 0.1 bit/sample, 10240
//! samples per key = 1024 credited bits vs 320 needed (256 + 64, 90B 3.1.5.1.2), ~3.2x oversampling as in
//! jitterentropy. Output is a hash ratchet (out = H(key|"out"|ctr), then key = H(key|"next")), reseeded from
//! 10240 fresh samples every 8 requests. It is an extractor + ratchet, NOT a certified SP 800-90A DRBG.
use crate::port::rdtsc;
use crate::{kprint, kprintln};
use core::arch::x86_64::{__cpuid, _rdrand64_step};
use sha2::{Digest, Sha256};

const H_CLAIM_MILLI: u32 = 100; // 0.1 bit per 8-bit sample, claimed (see header)
const RCT_CUTOFF: u32 = 301;
const APT_W: u32 = 512;
const APT_CUTOFF: u32 = 507;
const STARTUP_N: usize = 4096; // estimated + health tested, and part of the seed
const BLOCK_N: usize = 10240; // samples per key (seed: STARTUP_N + the rest)
const RESEED_EVERY: u32 = 8;

pub fn has_rdrand() -> bool {
    let r = __cpuid(1);
    r.ecx & (1 << 30) != 0
}

#[target_feature(enable = "rdrand")]
unsafe fn rdrand64() -> Option<u64> {
    let mut v = 0u64;
    for _ in 0..32 {
        if _rdrand64_step(&mut v) == 1 {
            return Some(v);
        }
    }
    None
}

/// 32 bytes from RDRAND, or Err if it keeps failing (CF=0) or looks stuck (same value twice in a row).
fn rdrand32() -> Result<[u8; 32], ()> {
    let mut out = [0u8; 32];
    let mut last = None;
    for c in out.chunks_mut(8) {
        let v = unsafe { rdrand64() }.ok_or(())?;
        if last == Some(v) {
            return Err(());
        }
        last = Some(v);
        c.copy_from_slice(&v.to_le_bytes());
    }
    Ok(out)
}

// ---------------- noise source ----------------

/// 2 MiB: 4x the N570's 512 KiB L2, so the walk misses to DRAM (memory-controller/refresh timing is the
/// jitter jitterentropy relies on). A 256 KiB walk would sit in L2 on the N570 (M2.5 self-review).
const WALK_SZ: usize = 2 * 1024 * 1024;
static mut WALK: [u8; WALK_SZ] = [0; WALK_SZ];
static mut WALK_POS: usize = 0;

#[cfg(not(feature = "badjitter"))]
#[inline(always)]
fn tsc() -> u64 {
    rdtsc()
}
/// Negative control: a broken timer (constant TSC). Every delta is 0, the RCT must fire and SSH must refuse.
#[cfg(feature = "badjitter")]
#[inline(always)]
fn tsc() -> u64 {
    let _ = rdtsc();
    0x4e43_3131
}

static mut WINDOW: u64 = 0;

#[inline(always)]
fn touch(pos: &mut usize) {
    unsafe {
        let p = core::ptr::addr_of_mut!(WALK) as *mut u8;
        *pos = (*pos + 4160) % WALK_SZ;
        let v = core::ptr::read_volatile(p.add(*pos));
        core::ptr::write_volatile(p.add(*pos), v.wrapping_add(1));
    }
}

/// TSC granularity: smallest nonzero step between back-to-back reads (QEMU/TCG: 1000; real CPUs: tens).
/// None = the timer never moved (broken/constant): no jitter source at all.
fn tsc_granularity() -> Option<u64> {
    let mut g = u64::MAX;
    let mut last = tsc();
    for _ in 0..100_000 {
        let t = tsc();
        let d = t.wrapping_sub(last);
        if d != 0 && d < g {
            g = d;
        }
        last = t;
    }
    if g == u64::MAX { None } else { Some(g) }
}

/// One raw sample = how many walk steps (strided over 2 MiB) complete while the TSC advances by WINDOW
/// ticks (max(64 x granularity, 32768)), starting on a TSC edge. It races two clocks: execution speed (cache, bus, and in
/// QEMU the host's scheduling/caches) against the timer. A first attempt timed a FIXED walk instead; under
/// TCG the TSC moves in 1000-tick steps, so the delta was nearly constant and both the RCT and the in-kernel
/// estimate refused (proof/m25-short-walk-serial.log, proof/m25-fixedwalk-serial.log).
/// If no TSC edge is seen the raw reading is 0 (a dead timer reads as a constant, so the RCT fires).
fn raw_delta() -> u64 {
    let w = unsafe { WINDOW };
    let t0 = tsc();
    let mut start = t0;
    let mut spins = 0u32;
    while start == t0 {
        start = tsc();
        spins += 1;
        if spins > 1 << 16 {
            return 0;
        }
    }
    let mut pos = unsafe { WALK_POS };
    let mut n = 0u64;
    while tsc().wrapping_sub(start) < w && n < 1 << 20 {
        touch(&mut pos);
        n += 1;
    }
    unsafe { WALK_POS = pos };
    n
}

// ---------------- health tests (SP 800-90B 4.4) ----------------

struct Health {
    rct_last: u8,
    rct_n: u32,
    apt_first: u8,
    apt_n: u32,
    apt_i: u32,
    started: bool,
}

impl Health {
    const fn new() -> Self {
        Self { rct_last: 0, rct_n: 0, apt_first: 0, apt_n: 0, apt_i: 0, started: false }
    }
    /// Err(name) when a test fires.
    fn feed(&mut self, s: u8) -> Result<(), &'static str> {
        if self.started && s == self.rct_last {
            self.rct_n += 1;
            if self.rct_n >= RCT_CUTOFF {
                return Err("repetition count");
            }
        } else {
            self.rct_last = s;
            self.rct_n = 1;
        }
        if !self.started || self.apt_i == APT_W {
            self.apt_first = s;
            self.apt_n = 1;
            self.apt_i = 1;
        } else {
            self.apt_i += 1;
            if s == self.apt_first {
                self.apt_n += 1;
                if self.apt_n >= APT_CUTOFF {
                    return Err("adaptive proportion");
                }
            }
        }
        self.started = true;
        Ok(())
    }
}

// ---------------- pool ----------------

static mut HEALTH: Health = Health::new();
static mut KEY: [u8; 32] = [0; 32];
static mut CTR: u64 = 0;
static mut CALLS: u32 = 0;
static mut RESEEDS: u32 = 0;
static mut SAMPLES: u64 = 0;
static mut SEEDED: bool = false;
static mut FAILED: bool = false;
static mut USE_RDRAND: bool = false;
static mut LOWB: [u8; STARTUP_N] = [0; STARTUP_N];

/// Collect `n` samples through the continuous health tests into `h`. Latches FAILED on any failure.
fn gather(h: &mut Sha256, n: usize, keep_low: bool) -> Result<(), &'static str> {
    for i in 0..n {
        let d = raw_delta();
        let s = d as u8;
        unsafe { SAMPLES += 1 };
        if let Err(e) = unsafe { (*core::ptr::addr_of_mut!(HEALTH)).feed(s) } {
            unsafe { FAILED = true };
            return Err(e);
        }
        if keep_low {
            unsafe { LOWB[i] = s };
        }
        h.update(d.to_le_bytes());
    }
    Ok(())
}

fn sqrt(x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut g = if x > 1.0 { x } else { 1.0 };
    for _ in 0..60 {
        g = 0.5 * (g + x / g);
    }
    g
}

fn log2(x: f64) -> f64 {
    let b = x.to_bits();
    let e = ((b >> 52) & 0x7ff) as i64 - 1023;
    let m = f64::from_bits((b & 0x000f_ffff_ffff_ffff) | (1023u64 << 52)); // [1, 2)
    let y = (m - 1.0) / (m + 1.0);
    let (mut s, mut t) = (0.0, y);
    let mut k = 1.0;
    while k < 60.0 {
        s += t / k;
        t *= y * y;
        k += 2.0;
    }
    e as f64 + 2.0 * s / core::f64::consts::LN_2
}

static mut TRANS: [[u16; 256]; 256] = [[0; 256]; 256];

/// Lag-1 bound: max over previous values a (>= 20 successors) of the 99% upper bound on max_b P(b | a).
/// Not a 90B estimator; a cheap guard against the serial correlation MCV (an IID estimate) cannot see.
fn lag1_milli(samples: &[u8]) -> u32 {
    let t = unsafe { &mut *core::ptr::addr_of_mut!(TRANS) };
    for row in t.iter_mut() {
        row.fill(0);
    }
    for w in samples.windows(2) {
        t[w[0] as usize][w[1] as usize] += 1;
    }
    let mut worst = 0.0f64;
    for row in t.iter() {
        let n: u32 = row.iter().map(|&x| x as u32).sum();
        if n >= 20 {
            let p = *row.iter().max().unwrap() as f64 / n as f64;
            let pu = (p + 2.576 * sqrt(p * (1.0 - p) / (n as f64 - 1.0))).min(1.0);
            if pu > worst {
                worst = pu;
            }
        }
    }
    if worst == 0.0 {
        return 8000;
    }
    (-log2(worst) * 1000.0) as u32
}

/// SP 800-90B 6.3.1 most-common-value estimate, in milli-bits per 8-bit sample.
fn mcv_milli(samples: &[u8]) -> (u32, u32, u32) {
    let mut c = [0u32; 256];
    for &s in samples {
        c[s as usize] += 1;
    }
    let max = *c.iter().max().unwrap();
    let distinct = c.iter().filter(|&&x| x > 0).count() as u32;
    let n = samples.len() as f64;
    let p = max as f64 / n;
    let pu = (p + 2.576 * sqrt(p * (1.0 - p) / (n - 1.0))).min(1.0);
    ((-log2(pu) * 1000.0) as u32, max, distinct)
}

/// `rng=dump`: print a SEPARATE batch of STARTUP_N raw samples (never used for keys) for offline estimation
/// (tools/entropy-est.py, or NIST SP800-90B ea_non_iid). Printed before the health tests so a failing source
/// can still be measured. Full deltas for the first 64, then the 8-bit samples.
fn dump_raw() {
    kprint!("rawdelta:");
    for _ in 0..64 {
        kprint!(" {}", raw_delta());
    }
    kprintln!("");
    let mut line = [0u8; 64];
    for i in 0..STARTUP_N / 64 {
        for b in line.iter_mut() {
            *b = raw_delta() as u8;
        }
        kprint!("rawjit {:04}:", i * 64);
        for b in line {
            kprint!(" {:02x}", b);
        }
        kprintln!("");
    }
}

/// Startup: health-test STARTUP_N samples, estimate min-entropy, seed the key. Err = do not run SSH.
pub fn init(dump: bool) -> Result<(), ()> {
    unsafe {
        if SEEDED {
            return Ok(());
        }
        if FAILED {
            return Err(());
        }
    }
    // A dead timer is not refused here: the samples then all read 0 and the health tests (the one authority
    // on the source) must refuse. That is the path the `badjitter` negative control exercises.
    let g = match tsc_granularity() {
        Some(g) => g,
        None => {
            kprintln!("rng: TSC never advanced in 100000 reads (dead timer?)");
            1
        }
    };
    // 64 timer steps, but never under 32768 ticks (~20 us on the N570): on a cycle-exact TSC, 64 x granularity
    // would be a window of a few DRAM accesses and the count close to constant.
    let w = (64 * g).max(32768);
    unsafe { WINDOW = w };
    kprintln!("rng: TSC granularity {} ticks, sample window {} ticks", g, w);
    if dump {
        dump_raw();
    }
    let mut h = Sha256::new();
    h.update(b"nc110-m2.5 seed");
    let t0 = rdtsc();
    if let Err(e) = gather(&mut h, STARTUP_N, true) {
        kprintln!("rng: jitter startup health test FAILED ({}) -> refusing to run SSH", e);
        return Err(());
    }
    let cycles = rdtsc().wrapping_sub(t0);
    let low = unsafe { &*core::ptr::addr_of!(LOWB) };
    let (hm, max, distinct) = mcv_milli(low);
    let hl = lag1_milli(low);
    kprintln!(
        "rng: jitter startup {} samples passed RCT(cut {}) + APT(W {}, cut {}); {} TSC ticks/sample",
        STARTUP_N, RCT_CUTOFF, APT_W, APT_CUTOFF, cycles / STARTUP_N as u64
    );
    kprintln!(
        "rng: min-entropy per 8-bit sample: MCV {}.{:03} (max {}/{}, {} distinct), lag-1 {}.{:03}; claimed {}.{:03}",
        hm / 1000, hm % 1000, max, STARTUP_N, distinct, hl / 1000, hl % 1000, H_CLAIM_MILLI / 1000, H_CLAIM_MILLI % 1000
    );
    if hm.min(hl) < H_CLAIM_MILLI {
        unsafe { FAILED = true };
        kprintln!("rng: estimate below the claimed 0.1 bit/sample -> refusing to run SSH");
        return Err(());
    }
    if let Err(e) = gather(&mut h, BLOCK_N - STARTUP_N, false) {
        kprintln!("rng: jitter health test FAILED while seeding ({}) -> refusing to run SSH", e);
        return Err(());
    }
    if has_rdrand() {
        match rdrand32() {
            Ok(r) => {
                h.update(r);
                unsafe { USE_RDRAND = true };
                kprintln!("rng: RDRAND present, mixed in (jitter alone already passed)");
            }
            Err(()) => {
                unsafe { FAILED = true };
                kprintln!("rng: RDRAND present but failing -> refusing to run SSH");
                return Err(());
            }
        }
    } else {
        kprintln!("rng: no RDRAND (as on the Atom N570): jitter is the only source");
    }
    unsafe {
        KEY = h.finalize().into();
        SEEDED = true;
    }
    Ok(())
}

/// Fill `buf` from the ratchet. Errors if not seeded, if a health test ever failed, or if a reseed fails.
pub fn fill(buf: &mut [u8]) -> Result<(), ()> {
    unsafe {
        if !SEEDED || FAILED {
            return Err(());
        }
        CALLS += 1;
        if CALLS % RESEED_EVERY == 0 {
            let mut h = Sha256::new();
            h.update(b"reseed");
            h.update(KEY);
            if gather(&mut h, BLOCK_N, false).is_err() {
                return Err(());
            }
            RESEEDS += 1;
            KEY = h.finalize().into();
        }
        if USE_RDRAND {
            let r = rdrand32().map_err(|_| FAILED = true)?;
            let mut h = Sha256::new();
            h.update(b"rdrand");
            h.update(KEY);
            h.update(r);
            KEY = h.finalize().into();
        }
        for chunk in buf.chunks_mut(32) {
            let mut h = Sha256::new();
            h.update(b"out");
            h.update(KEY);
            h.update(CTR.to_le_bytes());
            CTR += 1;
            chunk.copy_from_slice(&h.finalize()[..chunk.len()]);
        }
        let mut h = Sha256::new();
        h.update(b"next");
        h.update(KEY);
        KEY = h.finalize().into();
    }
    Ok(())
}

/// (requests served, reseeds, raw samples taken (all health-tested), health failure latched)
/// bmctl `rng <n>`: fresh raw 8-bit samples (low byte of each walk count), the same values the health tests and
/// estimators see. Never used for keys. For offline SP 800-90B assessment of the real board (tools/entropy-est.py).
pub fn raw_bytes(out: &mut [u8]) {
    for b in out.iter_mut() {
        *b = raw_delta() as u8;
    }
}

pub fn stats() -> (u32, u32, u64, bool) {
    unsafe { (CALLS, RESEEDS, SAMPLES, FAILED) }
}

#[no_mangle]
unsafe extern "Rust" fn __getrandom_v03_custom(dest: *mut u8, len: usize) -> Result<(), getrandom::Error> {
    core::ptr::write_bytes(dest, 0, len);
    let buf = core::slice::from_raw_parts_mut(dest, len);
    fill(buf).map_err(|_| getrandom::Error::UNSUPPORTED)
}
