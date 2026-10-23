//! ACPI embedded controller, read-only, for the battery line in the footer. Ports and register map come from the
//! NC110's own DSDT (proof/acpi/nc110-DSDT.dsl, H_EC): data 0x62, command/status 0x66, read command 0x80;
//! 0x80 bit 0 battery present, bit 2 AC; 0xA2 remaining capacity, 0xA4 present rate (mA, signed), 0xA6 voltage (mV),
//! 0xB2 last full charge (16-bit, big-endian). _BIF power unit 1: mA / mAh.
use crate::port::{inb, outb};

const DATA: u16 = 0x62;
const SC: u16 = 0x66;

fn wait(mask: u8, set: bool) -> bool {
    for _ in 0..20_000 {
        let s = unsafe { inb(SC) };
        if (s & mask != 0) == set {
            return true;
        }
    }
    false // bounded: a busy or absent EC never stalls the loop
}

pub fn read(addr: u8) -> Option<u8> {
    unsafe {
        wait(0x02, false).then_some(())?; // IBF clear
        outb(SC, 0x80);
        wait(0x02, false).then_some(())?;
        outb(DATA, addr);
        wait(0x01, true).then_some(())?; // OBF set
        Some(inb(DATA))
    }
}

/// 16-bit EC battery fields are big-endian: the DSDT byte-swaps every one it reads (B1RA, B1PR, B1VO, B1DF...).
fn read16(addr: u8) -> Option<u16> {
    Some((read(addr)? as u16) << 8 | read(addr + 1)? as u16)
}

/// (percent, on AC, charging) or None if no battery / no EC answer.
pub fn battery() -> Option<(u8, bool, bool)> {
    let flags = read(0x80)?;
    if flags & 1 == 0 {
        return None;
    }
    let (rem, full) = (read16(0xA2)?, read16(0xB2)?);
    if full == 0 || full == 0xFFFF || rem == 0xFFFF {
        return None;
    }
    let st = read(0x84)?;
    Some(((rem as u32 * 100 / full as u32).min(100) as u8, flags & 4 != 0, st & 2 != 0))
}

/// Battery power in milliwatts, positive while discharging (current x voltage from _BST's 0xA4 / 0xA6); None without a
/// battery or an EC answer. On AC it reads the charge current instead, so the footer shows it only off AC.
pub fn power_mw() -> Option<i32> {
    if read(0x80)? & 1 == 0 {
        return None;
    }
    let (ma, mv) = (read16(0xA4)? as i16 as i32, read16(0xA6)?);
    (mv != 0xFFFF && ma != -1).then(|| ma.abs() * mv as i32 / 1000)
}
