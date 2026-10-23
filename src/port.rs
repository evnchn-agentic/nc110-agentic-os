use core::arch::asm;

pub unsafe fn outb(p: u16, v: u8) { asm!("out dx, al", in("dx") p, in("al") v, options(nostack, preserves_flags)); }
pub unsafe fn outw(p: u16, v: u16) { asm!("out dx, ax", in("dx") p, in("ax") v, options(nostack, preserves_flags)); }
pub unsafe fn outl(p: u16, v: u32) { asm!("out dx, eax", in("dx") p, in("eax") v, options(nostack, preserves_flags)); }
pub unsafe fn inb(p: u16) -> u8 { let v: u8; asm!("in al, dx", out("al") v, in("dx") p, options(nostack, preserves_flags)); v }
pub unsafe fn inw(p: u16) -> u16 { let v: u16; asm!("in ax, dx", out("ax") v, in("dx") p, options(nostack, preserves_flags)); v }
pub unsafe fn inl(p: u16) -> u32 { let v: u32; asm!("in eax, dx", out("eax") v, in("dx") p, options(nostack, preserves_flags)); v }

pub fn rdtsc() -> u64 {
    let (lo, hi): (u32, u32);
    unsafe { asm!("rdtsc", out("eax") lo, out("edx") hi, options(nostack, nomem)) };
    ((hi as u64) << 32) | lo as u64
}
