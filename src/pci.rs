//! PCI config mechanism #1 (0xCF8/0xCFC), with a bridge walk (M2.5: the NC110's RTL8101E is expected to sit
//! behind a PCIe root port, i.e. on a secondary bus). The walk only READS config space. The one config write
//! in this kernel goes to the matched NIC function (command register), see nic.rs.
use crate::port::{inl, outl};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Bdf {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
}

impl core::fmt::Display for Bdf {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{:02x}:{:02x}.{}", self.bus, self.dev, self.func)
    }
}

fn addr(b: Bdf, off: u8) -> u32 {
    0x8000_0000 | (b.bus as u32) << 16 | (b.dev as u32) << 11 | (b.func as u32) << 8 | (off as u32 & 0xFC)
}

pub fn read32(b: Bdf, off: u8) -> u32 {
    unsafe {
        outl(0xCF8, addr(b, off));
        inl(0xCFC)
    }
}

pub fn write32(b: Bdf, off: u8, v: u32) {
    unsafe {
        outl(0xCF8, addr(b, off));
        outl(0xCFC, v);
    }
}

/// Visit every present function: bus 0 first, then the secondary bus of every type-1 (bridge) header,
/// depth first. `visit` returns true to stop. Bounded: at most 32 buses visited, depth 8.
pub fn walk(visit: &mut dyn FnMut(Bdf, u32) -> bool) -> bool {
    let mut seen = [false; 256];
    let mut budget = 32;
    walk_bus(0, 0, &mut seen, &mut budget, visit)
}

fn walk_bus(bus: u8, depth: u32, seen: &mut [bool; 256], budget: &mut u32, visit: &mut dyn FnMut(Bdf, u32) -> bool) -> bool {
    if seen[bus as usize] || depth > 8 || *budget == 0 {
        return false;
    }
    seen[bus as usize] = true;
    *budget -= 1;
    for dev in 0..32u8 {
        let f0 = Bdf { bus, dev, func: 0 };
        if read32(f0, 0) & 0xFFFF == 0xFFFF {
            continue;
        }
        let multi = (read32(f0, 0x0C) >> 16) & 0x80 != 0;
        for func in 0..if multi { 8u8 } else { 1 } {
            let b = Bdf { bus, dev, func };
            let id = read32(b, 0);
            if id & 0xFFFF == 0xFFFF {
                continue;
            }
            if visit(b, id) {
                return true;
            }
            let htype = (read32(b, 0x0C) >> 16) & 0x7F;
            if htype == 1 {
                let sec = ((read32(b, 0x18) >> 8) & 0xFF) as u8;
                if sec != 0 && sec != bus && walk_bus(sec, depth + 1, seen, budget, visit) {
                    return true;
                }
            }
        }
    }
    false
}

/// First function matching vendor:device anywhere in the walked hierarchy.
pub fn find(vendor: u16, device: u16) -> Option<Bdf> {
    let want = (device as u32) << 16 | vendor as u32;
    let mut hit = None;
    walk(&mut |b, id| {
        if id == want {
            hit = Some(b);
            true
        } else {
            false
        }
    });
    hit
}

/// (base, is_64bit) of the first MEMORY BAR of a type-0 function (mirrors r8169's "first MMIO region",
/// r8169_main.c:5408). Read-only: no BAR sizing writes.
pub fn first_mem_bar(b: Bdf) -> Option<(u64, bool)> {
    let mut i = 0u8;
    while i < 6 {
        let off = 0x10 + 4 * i;
        let lo = read32(b, off);
        if lo != 0 && lo & 1 == 0 { // 0 = unimplemented slot (RTL8102E BAR1), not a memory BAR at 0
            let ty = (lo >> 1) & 3;
            let base = (lo & !0xF) as u64;
            if ty == 2 {
                if i == 5 {
                    return None;
                }
                let hi = read32(b, off + 4) as u64;
                return Some((base | hi << 32, true));
            }
            return Some((base, false));
        }
        i += 1;
    }
    None
}

/// Memory BAR bases of every mass-storage function (class 0x01: IDE/SATA/AHCI). Read-only; used so the NIC's
/// MMIO window is refused if it is anywhere near the disk controller's.
pub fn storage_mem_bars(out: &mut [u64; 16]) -> usize {
    let mut n = 0;
    walk(&mut |b, _| {
        let class = read32(b, 0x08) >> 24;
        let htype = (read32(b, 0x0C) >> 16) & 0x7F;
        if class == 0x01 && htype == 0 {
            let mut i = 0u8;
            while i < 6 {
                let lo = read32(b, 0x10 + 4 * i);
                if lo & 1 == 0 {
                    let mut base = (lo & !0xF) as u64;
                    if (lo >> 1) & 3 == 2 && i < 5 {
                        base |= (read32(b, 0x10 + 4 * (i + 1)) as u64) << 32;
                        i += 1;
                    }
                    if base != 0 && n < out.len() {
                        out[n] = base;
                        n += 1;
                    }
                }
                i += 1;
            }
        }
        false
    });
    n
}

pub const fn b0(dev: u8, func: u8) -> Bdf {
    Bdf { bus: 0, dev, func }
}
