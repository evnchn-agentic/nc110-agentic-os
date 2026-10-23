//! ACPI tables, read-only: find the RSDP in the BIOS area, walk the RSDT, and hand table bytes to bmctl (`acpi <sig>
//! <off>`, e.g. FACP, DSDT, FACS), so an agent can read the firmware's description of the battery, the EC and the
//! sleep states without booting Linux. Nothing is written.
use core::ptr::read_volatile;

fn rd32(a: u64) -> u32 {
    unsafe { read_volatile(a as *const u32) }
}

fn sum_ok(a: u64, len: usize) -> bool {
    (0..len).fold(0u8, |s, i| s.wrapping_add(unsafe { read_volatile((a + i as u64) as *const u8) })) == 0
}

fn rsdt() -> Option<u64> {
    for a in (0xE0000u64..0x100000).step_by(16) {
        if unsafe { read_volatile(a as *const [u8; 8]) } == *b"RSD PTR " && sum_ok(a, 20) {
            return Some(rd32(a + 16) as u64);
        }
    }
    None
}

/// (address, length) of the table with signature `sig`; "DSDT" and "FACS" are found through the FADT.
pub fn table(sig: &[u8; 4]) -> Option<(u64, u32)> {
    let r = rsdt()?;
    if unsafe { read_volatile(r as *const [u8; 4]) } != *b"RSDT" {
        return None;
    }
    let n = (rd32(r + 4) as u64).saturating_sub(36) / 4;
    let mut fadt = None;
    for i in 0..n.min(64) {
        let t = rd32(r + 36 + 4 * i) as u64;
        let s = unsafe { read_volatile(t as *const [u8; 4]) };
        if s == *sig {
            return Some((t, rd32(t + 4)));
        }
        if s == *b"FACP" {
            fadt = Some(t);
        }
    }
    let f = fadt?;
    let t = match sig {
        b"DSDT" => rd32(f + 40) as u64,
        b"FACS" => rd32(f + 36) as u64,
        _ => return None,
    };
    let len = if sig == b"FACS" { rd32(t + 4) } else { rd32(t + 4) };
    (unsafe { read_volatile(t as *const [u8; 4]) } == *sig).then_some((t, len))
}

/// Up to `out.len()` bytes of table `sig` from offset `off`; the table length, or None if absent.
pub fn read(sig: &[u8; 4], off: usize, out: &mut [u8]) -> Option<(u32, usize)> {
    let (a, len) = table(sig)?;
    let len = len.min(1 << 20);
    let n = out.len().min((len as usize).saturating_sub(off));
    for (i, b) in out[..n].iter_mut().enumerate() {
        *b = unsafe { read_volatile((a + (off + i) as u64) as *const u8) };
    }
    Some((len, n))
}

/// Zero the FACS waking vectors (offsets 12 and 24) if set; returns the old (32-bit, 64-bit) values. Only an OS about
/// to sleep sets them; one left over (the 2026-10-21 01:40 S3 attempt left 0x6000) may make the firmware treat a later
/// reset as a resume from S3.
pub fn clear_waking_vector() -> Option<(u32, u64)> {
    let (f, _) = table(b"FACS")?;
    let (v, x) = unsafe { (read_volatile((f + 12) as *const u32), read_volatile((f + 24) as *const u64)) };
    if v != 0 || x != 0 {
        unsafe {
            core::ptr::write_volatile((f + 12) as *mut u32, 0);
            core::ptr::write_volatile((f + 24) as *mut u64, 0);
        }
    }
    Some((v, x))
}
