//! In-RAM chain loader, driven over bmctl (the DS OTA equivalent: nothing is written to /boot).
//! `lbeg <size> <sha256>` opens a staging buffer at STAGE; binary `ldat` ++ u32 offset ++ bytes appends in order;
//! `lcom <cmdline>` verifies length + SHA-256 + the multiboot header (a.out kludge, flag bit 16, as GRUB applies it),
//! checks every destination range, builds a multiboot info block (cmdline, the ORIGINAL GRUB memory map, and a
//! rebuilt 72-byte key module with this boot's seed), quiesces the NIC and jumps through the trampoline in boot.s,
//! which turns paging off, copies the image to load_addr, zeroes its bss and enters it like GRUB would.
//! `lcomlinux <cmdline>` instead boots the staged image as an i386 Linux bzImage (32-bit boot protocol, no initrd):
//! the route from the agent OS back to Slot A Linux without hands (Codex agent-OS review finding 3).
//! The TCO stays armed across the hop (the attended successor re-arms it in kmain). Any refusal leaves us running.
use crate::kprintln;
use crate::mb;
use crate::nic::Rtl;
use sha2::{Digest, Sha256};

pub const STAGE: u64 = 0x0400_0000; // 64 MiB
pub const CAP: u64 = 0x0200_0000; // 32 MiB (bzImage-sized; STAGE + CAP = 96 MiB < 128 MiB, so q35 -m 128 works)
const TRAMP: u64 = 0x8000; // trampoline code + its stack (< 0xC00)
const PARAMS: u64 = 0x8C00;
const INFO: u64 = 0x8D00;
const MODENT: u64 = 0x8E80;
const CMDLINE: u64 = 0x8F00; // <= 255 bytes + NUL
const MODULE: u64 = 0x9000; // page-aligned (our header asks for it: flag bit 0)
const MMAP: u64 = 0x9100;
const LOW_END: u64 = 0xA000;
const MMAP_MAX: usize = (LOW_END - MMAP) as usize;

/// GRUB's boot metadata, copied out at boot (before any staging could overwrite it). (flags, mem_lower, mem_upper)
static mut ORIG: (u32, u32, u32) = (0, 0, 0);
static mut ORIG_MMAP: [u8; MMAP_MAX] = [0; MMAP_MAX];
static mut ORIG_MMAP_LEN: usize = 0;
static mut ORIG_MMAP_TOO_BIG: bool = false;
/// GRUB's framebuffer fields (multiboot info 88..120, colour info at 112) if it set a video mode: the mode survives a chain-load, so the
/// next kernel gets the same description (header flag bit 2 = video mode wanted).
static mut ORIG_FB: Option<[u8; 32]> = None;

/// Call once, early, with the multiboot info pointer GRUB (or a previous loader) gave us.
pub fn snapshot_boot_info(info: u64) {
    if info == 0 {
        return;
    }
    unsafe {
        let r32 = |a: u64| core::ptr::read_volatile(a as *const u32);
        let f = r32(info);
        ORIG = (f, if f & 1 != 0 { r32(info + 4) } else { 0 }, if f & 1 != 0 { r32(info + 8) } else { 0 });
        if f & (1 << 12) != 0 {
            let mut b = [0u8; 32];
            core::ptr::copy((info + 88) as *const u8, b.as_mut_ptr(), 32);
            ORIG_FB = Some(b);
        }
        if f & (1 << 6) != 0 {
            let (l, a) = (r32(info + 44) as usize, r32(info + 48) as u64);
            if l <= MMAP_MAX {
                core::ptr::copy(a as *const u8, core::ptr::addr_of_mut!(ORIG_MMAP) as *mut u8, l);
                ORIG_MMAP_LEN = l;
            } else {
                ORIG_MMAP_TOO_BIG = true;
            }
        }
    }
}

extern "C" {
    static tramp_start: u8;
    static tramp_end: u8;
    static __load_start: u8;
    static __bss_end: u8;
}

pub struct Stage {
    size: u64,
    sha: [u8; 32],
    got: u64,
}

fn overlaps(a: u64, al: u64, b: u64, bl: u64) -> bool {
    a < b.saturating_add(bl) && b < a.saturating_add(al)
}

fn kernel_image() -> (u64, u64) {
    let (s, e) = (core::ptr::addr_of!(__load_start) as u64, core::ptr::addr_of!(__bss_end) as u64);
    (s, e - s)
}

pub fn begin(size: u64, sha_hex: &str) -> Result<Stage, &'static str> {
    if size == 0 || size > CAP {
        return Err("size 1..32 MiB");
    }
    let (ks, kl) = kernel_image();
    if !mb::ram_contains(STAGE, size) || overlaps(STAGE, size, ks, kl) {
        return Err("staging area not free RAM");
    }
    if sha_hex.len() != 64 || !sha_hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("sha256: 64 hex chars");
    }
    let mut sha = [0u8; 32];
    for (i, b) in sha.iter_mut().enumerate() {
        *b = u8::from_str_radix(&sha_hex[2 * i..2 * i + 2], 16).map_err(|_| "sha256: hex")?;
    }
    Ok(Stage { size, sha, got: 0 })
}

pub fn data(st: &mut Stage, off: u64, bytes: &[u8]) -> Result<u64, &'static str> {
    if off != st.got {
        return Err("out of order (send chunks in sequence)");
    }
    let end = off.checked_add(bytes.len() as u64).ok_or("overflow")?;
    if end > st.size {
        return Err("past announced size");
    }
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), (STAGE + off) as *mut u8, bytes.len()) };
    st.got = end;
    Ok(end)
}

/// Everything checked, nothing moved yet. Returns (src, dst, len, bss_len, entry).
fn plan(st: &Stage) -> Result<(u64, u64, u64, u64, u64), &'static str> {
    if st.got != st.size {
        return Err("incomplete image");
    }
    let img = unsafe { core::slice::from_raw_parts(STAGE as *const u8, st.size as usize) };
    let d: [u8; 32] = Sha256::digest(img).into();
    if d != st.sha {
        return Err("sha256 mismatch");
    }
    let rd = |o: usize| u32::from_le_bytes(img[o..o + 4].try_into().unwrap());
    let lim = img.len().min(8192);
    let hoff = (0..lim.saturating_sub(31)).step_by(4).find(|&o| {
        rd(o) == 0x1BAD_B002 && rd(o).wrapping_add(rd(o + 4)).wrapping_add(rd(o + 8)) == 0
    }).ok_or("no multiboot header in the first 8 KiB")?;
    let flags = rd(hoff + 4);
    if flags & (1 << 16) == 0 {
        return Err("multiboot header lacks the address kludge (flag 16)");
    }
    if flags & 0xFFF8 != 0 {
        return Err("multiboot header requires features this loader does not provide (bits 3..15)");
    }
    let (haddr, laddr, lend, bend, entry) =
        (rd(hoff + 12) as u64, rd(hoff + 16) as u64, rd(hoff + 20) as u64, rd(hoff + 24) as u64, rd(hoff + 28) as u64);
    if laddr > haddr || lend <= laddr || bend < lend || entry < laddr || entry >= lend {
        return Err("inconsistent header addresses");
    }
    let foff = (hoff as u64).checked_sub(haddr - laddr).ok_or("header offset")?;
    let len = lend - laddr;
    if foff.checked_add(len).is_none_or(|e| e > st.size) {
        return Err("image shorter than load_end - load_addr");
    }
    if laddr < 0x10_0000 || bend > STAGE || !mb::ram_contains(laddr, bend - laddr) {
        return Err("target range not RAM in [1 MiB, staging)");
    }
    if overlaps(laddr, bend - laddr, TRAMP, LOW_END - TRAMP) {
        return Err("target overlaps the trampoline/info pages");
    }
    Ok((STAGE + foff, laddr, len, bend - lend, entry))
}

pub struct Plan {
    params: [u64; 7], // src, dst, len, bss, entry, ebx (info / boot_params), mode (0 multiboot, 1 Linux)
}

/// All checks plus writing the low pages (trampoline, info, module, cmdline, mmap). Nothing of ours is touched:
/// the low pages are outside the kernel image; GRUB's info/mmap come from the boot-time snapshot.
pub fn prepare(st: &Stage, cmdline: &str, seed: &[u8; 32]) -> Result<Plan, &'static str> {
    let (src, dst, len, bss, entry) = plan(st)?;
    let tl = (core::ptr::addr_of!(tramp_end) as u64) - (core::ptr::addr_of!(tramp_start) as u64);
    if tl > PARAMS - TRAMP || !mb::ram_contains(TRAMP, LOW_END - TRAMP) {
        return Err("low pages 0x8000-0xA000 not usable");
    }
    if cmdline.len() > 255 {
        return Err("cmdline > 255");
    }
    unsafe {
        if ORIG_MMAP_TOO_BIG {
            return Err("GRUB memory map larger than the loader's buffer: refusing rather than dropping it");
        }
        let (oflags, mlo, mhi) = ORIG;
        let ml = ORIG_MMAP_LEN;
        let mmap = &*core::ptr::addr_of!(ORIG_MMAP);
        let w32 = |a: u64, v: u32| core::ptr::write_volatile(a as *mut u32, v);
        core::ptr::write_bytes(TRAMP as *mut u8, 0, (LOW_END - TRAMP) as usize);
        core::ptr::copy_nonoverlapping(core::ptr::addr_of!(tramp_start), TRAMP as *mut u8, tl as usize);
        // key module: MAGIC + seed + public key, exactly the 72 bytes GRUB's `module` line gave us
        core::ptr::copy_nonoverlapping(mb::MAGIC.as_ptr(), MODULE as *mut u8, 8);
        core::ptr::copy_nonoverlapping(seed.as_ptr(), (MODULE + 8) as *mut u8, 32);
        let publ = sunset::ed25519_dalek::SigningKey::from_bytes(seed).verifying_key().to_bytes();
        core::ptr::copy_nonoverlapping(publ.as_ptr(), (MODULE + 40) as *mut u8, 32);
        w32(MODENT, MODULE as u32);
        w32(MODENT + 4, (MODULE + 72) as u32);
        core::ptr::copy_nonoverlapping(cmdline.as_ptr(), CMDLINE as *mut u8, cmdline.len());
        // multiboot info: flags 0 (mem), 2 (cmdline), 3 (mods), 6 (GRUB's memory map, verbatim)
        let mut flags = (1 << 2) | (1 << 3);
        if oflags & 1 != 0 {
            flags |= 1;
            w32(INFO + 4, mlo);
            w32(INFO + 8, mhi);
        }
        if ml > 0 {
            core::ptr::copy_nonoverlapping(mmap.as_ptr(), MMAP as *mut u8, ml);
            flags |= 1 << 6;
            w32(INFO + 44, ml as u32);
            w32(INFO + 48, MMAP as u32);
        }
        if let Some(fb) = *core::ptr::addr_of!(ORIG_FB) {
            core::ptr::copy_nonoverlapping(fb.as_ptr(), (INFO + 88) as *mut u8, 32);
            flags |= 1 << 12;
        }
        w32(INFO, flags);
        w32(INFO + 16, CMDLINE as u32);
        w32(INFO + 20, 1);
        w32(INFO + 24, MODENT as u32);
    }
    kprintln!("loader: ready: {} B image -> {:#x}..{:#x} (+{} B bss), entry {:#x}, cmdline '{}'", st.size, dst, dst + len, bss, entry, cmdline);
    Ok(Plan { params: [src, dst, len, bss, entry, INFO, 0] })
}

const BP: u64 = 0x10000; // Linux boot_params (zero page)
const BP_CMD: u64 = 0x11000; // Linux command line

/// Linux 32-bit boot protocol (Documentation/arch/x86/boot.rst): protected-mode code goes to code32_start, a zero
/// page carries the setup header, the command line and an E820 map built from GRUB's multiboot map.
pub fn prepare_linux(st: &Stage, cmdline: &str) -> Result<Plan, &'static str> {
    if st.got != st.size {
        return Err("incomplete image");
    }
    let img = unsafe { core::slice::from_raw_parts(STAGE as *const u8, st.size as usize) };
    let d: [u8; 32] = Sha256::digest(img).into();
    if d != st.sha {
        return Err("sha256 mismatch");
    }
    if img.len() < 0x1000 || img[0x1fe..0x200] != [0x55, 0xAA] || &img[0x202..0x206] != b"HdrS" {
        return Err("not a bzImage (boot flag / HdrS)");
    }
    let r16 = |o: usize| u16::from_le_bytes([img[o], img[o + 1]]) as u64;
    let r32 = |o: usize| u32::from_le_bytes(img[o..o + 4].try_into().unwrap()) as u64;
    let ver = r16(0x206);
    if ver < 0x020a || img[0x211] & 1 == 0 {
        return Err("bzImage boot protocol < 2.10 or not LOADED_HIGH");
    }
    let setup = if img[0x1f1] == 0 { 4 } else { img[0x1f1] as u64 };
    let off = (setup + 1) * 512;
    // the header must cover every field read below (init_size ends at 0x264)
    if (0x202 + img[0x201] as usize) < 0x264 {
        return Err("setup header too short for the fields this loader uses");
    }
    let (dst, init_size, pref) = (r32(0x214), r32(0x260), r32(0x258) | r32(0x25c) << 32);
    let (kalign, reloc, cmax) = (r32(0x230), img[0x234] != 0, r32(0x238));
    let len = st.size.checked_sub(off).ok_or("image shorter than its setup")?;
    if dst != 0x10_0000 || dst + len > STAGE || !mb::ram_contains(dst, len) {
        return Err("protected-mode code must load at 1 MiB, below staging, in RAM");
    }
    // Where the kernel decompresses and runs (boot.rst): pref_address if not relocatable; if relocatable, the load
    // address aligned up to kernel_alignment, or pref_address. Both candidate intervals must be RAM below 4 GiB.
    if init_size == 0 || (reloc && !kalign.is_power_of_two()) {
        return Err("bad init_size / kernel_alignment");
    }
    // relocatable: align_up(max(load address, pref_address), kernel_alignment); fixed: pref_address
    let runat = if reloc {
        dst.max(pref).checked_add(kalign - 1).ok_or("alignment overflow")? & !(kalign - 1)
    } else {
        pref
    };
    for base in [runat] {
        match base.checked_add(init_size) {
            Some(end) if end <= 0x1_0000_0000 && mb::ram_contains(base, init_size) => {}
            _ => return Err("kernel's runtime range (init_size at pref/aligned address) not RAM below 4 GiB"),
        }
    }
    let run = runat;
    if cmdline.len() > 2047 || cmdline.len() as u64 > cmax || !mb::ram_contains(BP, 0x2000) {
        return Err("cmdline longer than 2047 / the kernel's cmdline_size, or boot_params pages not RAM");
    }
    let tl = (core::ptr::addr_of!(tramp_end) as u64) - (core::ptr::addr_of!(tramp_start) as u64);
    if tl > PARAMS - TRAMP || !mb::ram_contains(TRAMP, LOW_END - TRAMP) {
        return Err("low pages 0x8000-0xA000 not usable");
    }
    unsafe {
        if ORIG_MMAP_TOO_BIG || ORIG_MMAP_LEN == 0 {
            return Err("no usable GRUB memory map snapshot for the E820 table");
        }
        core::ptr::write_bytes(BP as *mut u8, 0, 0x2000);
        core::ptr::write_bytes(TRAMP as *mut u8, 0, (LOW_END - TRAMP) as usize);
        core::ptr::copy_nonoverlapping(core::ptr::addr_of!(tramp_start), TRAMP as *mut u8, tl as usize);
        let hend = (0x202 + img[0x201] as usize).min(0x290);
        core::ptr::copy_nonoverlapping(img.as_ptr().add(0x1f1), (BP + 0x1f1) as *mut u8, hend - 0x1f1);
        for o in [0x218u64, 0x21c, 0x250, 0x254] {
            core::ptr::write_volatile((BP + o) as *mut u32, 0); // no initrd, no setup_data chain
        }
        core::ptr::write_volatile((BP + 0x210) as *mut u8, 0xFF); // type_of_loader: undefined
        core::ptr::write_volatile((BP + 0x228) as *mut u32, BP_CMD as u32); // cmd_line_ptr
        core::ptr::copy_nonoverlapping(cmdline.as_ptr(), BP_CMD as *mut u8, cmdline.len());
        // E820 from the multiboot map: entries {size u32, base u64, len u64, type u32}, stride size + 4
        let mm = &*core::ptr::addr_of!(ORIG_MMAP);
        let (mut p, mut n) = (0usize, 0u64);
        while p < ORIG_MMAP_LEN {
            if p + 4 > ORIG_MMAP_LEN {
                return Err("GRUB memory map ends inside a record header");
            }
            let sz = u32::from_le_bytes(mm[p..p + 4].try_into().unwrap()) as usize;
            if sz < 20 || p + 4 + sz > ORIG_MMAP_LEN || n >= 128 {
                return Err("GRUB memory map record malformed (size < 20, overrun) or > 128 records");
            }
            let e = BP + 0x2d0 + 20 * n;
            core::ptr::copy_nonoverlapping(mm.as_ptr().add(p + 4), e as *mut u8, 20);
            n += 1;
            p += sz + 4;
        }
        core::ptr::write_volatile((BP + 0x1e8) as *mut u8, n as u8);
    }
    kprintln!("loader: Linux ready: bzImage proto {:#x}, {} B pm code -> {:#x}, init_size {:#x} at {:#x}, cmdline '{}'", ver, len, dst, init_size, run, cmdline);
    Ok(Plan { params: [STAGE + off, dst, len, 0, dst, BP, 1] })
}

/// Point of no return: NIC quiesced (reset completed), trampoline entered. The TCO stays armed across the hop.
/// If the NIC reset does not complete, its DMA may still target memory the new image would overwrite: hard reset
/// instead of jumping (GRUB default, the agent OS, then the crash-loop escape).
pub fn jump(p: &Plan, nic: &Rtl) -> ! {
    if let Err(e) = crate::hda::quiesce() {
        kprintln!("loader: {}: NOT jumping, hard reset", e);
        crate::wdt::hard_reset();
    }
    let _ = crate::iwl::quiesce();
    if let Err(e) = nic.quiesce() {
        kprintln!("loader: NIC quiesce failed ({}): NOT jumping, hard reset", e);
        crate::wdt::hard_reset();
    }
    unsafe {
        for (i, v) in p.params.iter().enumerate() {
            core::ptr::write_volatile((PARAMS + 8 * i as u64) as *mut u64, *v);
        }
        crate::wdt::pet();
        let f: extern "sysv64" fn(u64) -> ! = core::mem::transmute(TRAMP as usize);
        f(PARAMS)
    }
}
