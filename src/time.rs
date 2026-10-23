//! Monotonic milliseconds from the TSC, calibrated once against PIT channel 2 (gate via port 0x61).
use crate::port::{inb, outb, rdtsc};

static mut TSC_PER_MS: u64 = 1_000_000; // fallback guess (1 GHz) if calibration fails
static mut T0: u64 = 0;

pub fn init() -> u64 {
    unsafe {
        // PIT ch2, mode 0, lobyte/hibyte, count for 10 ms at 1.193182 MHz.
        const COUNT: u16 = 11932;
        let g = inb(0x61);
        outb(0x61, (g & 0xFD) | 0x01); // gate on, speaker off
        outb(0x43, 0b1011_0000);
        outb(0x42, COUNT as u8);
        outb(0x42, (COUNT >> 8) as u8);
        // Re-trigger: toggle gate low->high so mode 0 starts counting now.
        let g = inb(0x61);
        outb(0x61, g & 0xFE);
        outb(0x61, g | 0x01);
        let a = rdtsc();
        let mut spins: u64 = 0;
        while inb(0x61) & 0x20 == 0 && spins < 200_000_000 {
            spins += 1;
        }
        let b = rdtsc();
        if spins < 200_000_000 && b > a {
            TSC_PER_MS = ((b - a) / 10).max(1);
        }
        T0 = rdtsc();
        TSC_PER_MS
    }
}

/// ms the TSC had counted when `init` ran: time since the last CPU reset (firmware + GRUB + our early boot),
/// since a hard reset zeroes the TSC; after an in-RAM chain-load it also includes the previous kernel's run.
pub fn since_reset_ms() -> u64 {
    unsafe { T0 / TSC_PER_MS }
}

pub fn tsc_to_ms(t: u64) -> u64 {
    unsafe { t / TSC_PER_MS }
}

pub fn ready() -> bool {
    unsafe { T0 != 0 }
}

pub fn now_ms() -> i64 {
    unsafe { ((rdtsc() - T0) / TSC_PER_MS) as i64 }
}

/// Busy-wait at least `us` microseconds (TSC, after init()).
pub fn delay_us(us: u64) {
    unsafe {
        let t = rdtsc();
        let n = us * TSC_PER_MS / 1000 + 1;
        while rdtsc().wrapping_sub(t) < n {
            core::hint::spin_loop();
        }
    }
}

/// CMOS RTC hours and minutes (the NC110's RTC keeps local time: Linux there labels HKT as UTC). Waits out an update in
/// progress (bounded), handles BCD and 12-hour formats. None if the RTC never settles.
pub fn rtc_hm() -> Option<(u8, u8)> {
    use crate::port::{inb, outb};
    let rd = |r: u8| unsafe {
        outb(0x70, r);
        inb(0x71)
    };
    for _ in 0..1000 {
        if rd(0x0A) & 0x80 == 0 {
            let (m, h, b) = (rd(0x02), rd(0x04), rd(0x0B));
            let bcd = |v: u8| if b & 0x04 == 0 { (v & 0x0F) + (v >> 4) * 10 } else { v };
            let pm = h & 0x80 != 0;
            let mut hh = bcd(h & 0x7F);
            if b & 0x02 == 0 {
                hh = hh % 12 + if pm { 12 } else { 0 };
            }
            return Some((hh, bcd(m)));
        }
    }
    None
}

/// CMOS RTC as seconds since 1970 (the RTC holds local time; read as UTC). For timestamps that must only grow.
pub fn rtc_unix() -> Option<u64> {
    use crate::port::{inb, outb};
    let rd = |r: u8| unsafe {
        outb(0x70, r);
        inb(0x71)
    };
    for _ in 0..1000 {
        if rd(0x0A) & 0x80 == 0 {
            let b = rd(0x0B);
            let bcd = |v: u8| if b & 0x04 == 0 { (v & 0x0F) as u64 + (v >> 4) as u64 * 10 } else { v as u64 };
            let h = rd(0x04);
            let mut hh = bcd(h & 0x7F);
            if b & 0x02 == 0 {
                hh = hh % 12 + if h & 0x80 != 0 { 12 } else { 0 };
            }
            let (ss, mm, d, mo, y) = (bcd(rd(0x00)), bcd(rd(0x02)), bcd(rd(0x07)), bcd(rd(0x08)), 2000 + bcd(rd(0x09)));
            // days from civil (Howard Hinnant)
            let (y2, m2) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
            let era = y2 / 400;
            let yoe = y2 - era * 400;
            let doy = (153 * m2 + 2) / 5 + d - 1;
            let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
            let days = era * 146097 + doe - 719468;
            return Some(days * 86400 + hh * 3600 + mm * 60 + ss);
        }
    }
    None
}
