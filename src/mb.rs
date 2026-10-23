//! Multiboot v1 info: cmdline, memory map, and the client-key module (M2.5).
//!
//! The SSH client key is NOT compiled into the ELF. GRUB loads it with `module /baremetal/<file>` (QEMU:
//! `-initrd <file>`), so the key lives on /boot and can be rotated without a rebuild.
//! Module format (72 bytes): b"NC110K1\n" | ed25519 seed (32) | ed25519 public key (32).
//! The kernel derives the public key from the seed and refuses on mismatch (truncation / corruption / wrong
//! file), copies the seed into a static, then zeroes the module bytes in RAM.
use crate::kprintln;
use core::ptr::{read_volatile, write_volatile};

pub const MAGIC: &[u8; 8] = b"NC110K1\n";
const KEYMOD_LEN: u64 = 72;

static mut RAM: [(u64, u64); 32] = [(0, 0); 32];
static mut NRAM: usize = 0;
static mut HAVE_MAP: bool = false;

unsafe fn rd32(a: u64) -> u32 {
    read_volatile(a as *const u32)
}

/// Record available-RAM ranges (mmap type 1, or mem_lower/mem_upper as a fallback).
pub fn parse_memory(info: u64) {
    unsafe {
        if info == 0 {
            return;
        }
        let flags = rd32(info);
        if flags & (1 << 6) != 0 {
            let len = rd32(info + 44) as u64;
            let mut p = rd32(info + 48) as u64;
            let end = p + len;
            while p + 24 <= end && NRAM < 32 {
                let size = rd32(p) as u64;
                let base = read_volatile((p + 4) as *const u64);
                let l = read_volatile((p + 12) as *const u64);
                let ty = rd32(p + 20);
                if ty == 1 && l > 0 {
                    RAM[NRAM] = (base, l);
                    NRAM += 1;
                }
                p += size + 4;
            }
            HAVE_MAP = true;
        } else if flags & 1 != 0 {
            RAM[0] = (0, rd32(info + 4) as u64 * 1024);
            RAM[1] = (0x10_0000, rd32(info + 8) as u64 * 1024);
            NRAM = 2;
            HAVE_MAP = true;
        }
    }
}

/// True if [base, base+len) touches available RAM, or if no memory map was given (fail closed).
pub fn ram_overlaps(base: u64, len: u64) -> bool {
    unsafe {
        if !HAVE_MAP {
            return true;
        }
        let ram = &*core::ptr::addr_of!(RAM);
        ram[..NRAM].iter().any(|&(b, l)| base < b + l && b < base + len)
    }
}

/// True only if [base, base+len) lies entirely inside ONE available-RAM range of the multiboot map.
pub fn ram_contains(base: u64, len: u64) -> bool {
    let Some(end) = base.checked_add(len) else { return false };
    unsafe {
        let ram = &*core::ptr::addr_of!(RAM);
        HAVE_MAP && ram[..NRAM].iter().any(|&(b, l)| base >= b && end <= b.saturating_add(l))
    }
}

pub fn cmdline(info: u64) -> &'static str {
    unsafe {
        if info == 0 || rd32(info) & 4 == 0 {
            return "";
        }
        let p = rd32(info + 16) as *const u8;
        let mut n = 0;
        while n < 256 && *p.add(n) != 0 {
            n += 1;
        }
        core::str::from_utf8(core::slice::from_raw_parts(p, n)).unwrap_or("")
    }
}

/// Find the key module, validate it, copy the seed out, wipe the module. None (with a log line) if absent/bad.
pub fn take_client_key(info: u64) -> Option<[u8; 32]> {
    unsafe {
        if info == 0 || rd32(info) & (1 << 3) == 0 || rd32(info + 20) == 0 {
            kprintln!("keymod: no multiboot module");
            return None;
        }
        let count = rd32(info + 20).min(16) as u64;
        let mods = rd32(info + 24) as u64;
        for i in 0..count {
            let e = mods + 16 * i;
            let (start, end) = (rd32(e) as u64, rd32(e + 4) as u64);
            if end <= start || end - start != KEYMOD_LEN {
                continue;
            }
            let m = core::slice::from_raw_parts(start as *const u8, KEYMOD_LEN as usize);
            if &m[..8] != MAGIC {
                continue;
            }
            let mut seed = [0u8; 32];
            let mut publ = [0u8; 32];
            seed.copy_from_slice(&m[8..40]);
            publ.copy_from_slice(&m[40..72]);
            for j in 0..KEYMOD_LEN {
                write_volatile((start + j) as *mut u8, 0);
            }
            let derived = sunset::ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key().to_bytes();
            if derived != publ {
                kprintln!("keymod: module {} public key does not match its seed -> ignored (wiped)", i);
                return None;
            }
            kprintln!("keymod: client key from module {} at {:#x} (72 B), public key verified, module wiped", i, start);
            return Some(seed);
        }
        kprintln!("keymod: {} module(s), none is a 72-byte NC110K1 key", count);
        None
    }
}

static mut CFG_BUF: [u8; 4096] = [0; 4096];

/// The optional config module ("NC110CFG1\n" + `host`/`wg` lines, at most 4 KiB): copied out, then wiped (it holds
/// the WireGuard private key). Applied by bmctl::boot_config once the network is up.
pub fn take_config(info: u64) -> Option<&'static str> {
    unsafe {
        if info == 0 || rd32(info) & (1 << 3) == 0 {
            return None;
        }
        let mods = rd32(info + 24) as u64;
        for i in 0..rd32(info + 20).min(16) as u64 {
            let (start, end) = (rd32(mods + 16 * i) as u64, rd32(mods + 16 * i + 4) as u64);
            let n = end.saturating_sub(start) as usize;
            if !(10..=4096).contains(&n) || core::slice::from_raw_parts(start as *const u8, 10) != b"NC110CFG1\n" {
                continue;
            }
            let b = &mut *core::ptr::addr_of_mut!(CFG_BUF);
            b[..n].copy_from_slice(core::slice::from_raw_parts(start as *const u8, n));
            for j in 0..n as u64 {
                write_volatile((start + j) as *mut u8, 0);
            }
            kprintln!("cfgmod: config module {} ({} B) copied and wiped", i, n);
            return core::str::from_utf8(&b[10..n]).ok();
        }
        None
    }
}

/// Multiboot framebuffer info (flags bit 12) if GRUB set a direct-RGB video mode (framebuffer_type 1).
pub fn framebuffer(info: u64) -> Option<crate::console::Fb> {
    unsafe {
        if info == 0 || rd32(info) & (1 << 12) == 0 {
            return None;
        }
        let rd8 = |o: u64| read_volatile((info + o) as *const u8);
        if rd8(109) != 1 {
            return None; // indexed or EGA text: not ours
        }
        Some(crate::console::Fb {
            addr: read_volatile((info + 88) as *const u64),
            pitch: rd32(info + 96),
            w: rd32(info + 100),
            h: rd32(info + 104),
            bytes: rd8(108) as u32 / 8,
            rgb: [(rd8(112), rd8(113)), (rd8(114), rd8(115)), (rd8(116), rd8(117))], // GRUB aligns the colour union to 4: 112, not the spec's 110
        })
    }
}
