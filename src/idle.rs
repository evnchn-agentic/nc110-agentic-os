//! Idle: halt the CPU between polls instead of spinning. The kernel runs with interrupts off and polls everything, so
//! a nap is: arm the local APIC timer one-shot, `sti; hlt; cli`, and take its interrupt in a stub that only sends EOI.
//! Only that vector (and the APIC spurious vector) is present in the IDT: a CPU exception still finds no handler and
//! triple-faults into a reset, exactly as before this existed. The 8259 PICs are masked so no legacy IRQ lands on an
//! exception vector during a nap. If the APIC cannot be mapped uncached, `nap` does nothing (the old busy loop).
use core::arch::{asm, global_asm};

const VEC: usize = 0x40;
const SPURIOUS: usize = 0xFF;

#[derive(Clone, Copy)]
#[repr(C)]
struct Gate {
    lo: u16,
    sel: u16,
    ist: u8,
    attr: u8,
    mid: u16,
    hi: u32,
    zero: u32,
}

#[repr(C, packed)]
struct Idtr {
    limit: u16,
    base: u64,
}

static mut IDT: [Gate; 256] = [Gate { lo: 0, sel: 0, ist: 0, attr: 0, mid: 0, hi: 0, zero: 0 }; 256];
#[no_mangle]
static mut IDLE_EOI: u64 = 0; // address of the APIC EOI register, read by the stub
static mut BASE: u64 = 0;
static mut TICKS_PER_MS: u32 = 0;
static mut MWAIT: Option<u32> = None; // idle via MONITOR/MWAIT with this hint (deeper C-states) instead of hlt
static mut MON: u64 = 0;
static mut SLEPT: u64 = 0; // TSC ticks spent halted, for "CPU asleep N%"

/// TSC ticks spent halted since init (monotonic).
pub fn slept() -> u64 {
    unsafe { SLEPT }
}

/// Idle with MWAIT `hint` (Some) or plain hlt (None). Err if the CPU has no MONITOR/MWAIT.
pub fn set_mwait(hint: Option<u32>) -> Result<(), &'static str> {
    if hint.is_some() {
        let c: u32;
        unsafe { asm!("push rbx", "cpuid", "pop rbx", inout("eax") 1u32 => _, out("ecx") c, out("edx") _, options(nostack)) };
        if c & (1 << 3) == 0 {
            return Err("no MONITOR/MWAIT");
        }
    }
    unsafe { MWAIT = hint };
    Ok(())
}

global_asm!(
    ".global idle_isr_timer",
    "idle_isr_timer:",
    "push rax",
    "mov rax, qword ptr [rip + IDLE_EOI]",
    "mov dword ptr [rax], 0",
    "pop rax",
    "iretq",
    ".global idle_isr_spurious",
    "idle_isr_spurious:",
    "iretq",
);

extern "C" {
    fn idle_isr_timer();
    fn idle_isr_spurious();
}

fn wr(off: u64, v: u32) {
    unsafe { core::ptr::write_volatile((BASE + off) as *mut u32, v) }
}

fn rd(off: u64) -> u32 {
    unsafe { core::ptr::read_volatile((BASE + off) as *const u32) }
}

fn gate(f: unsafe extern "C" fn(), sel: u16) -> Gate {
    let a = f as usize as u64;
    Gate { lo: a as u16, sel, ist: 0, attr: 0x8E, mid: (a >> 16) as u16, hi: (a >> 32) as u32, zero: 0 }
}

/// Set up the APIC timer and the two-vector IDT; false = naps stay off (printed).
pub fn init() -> bool {
    let (lo, hi): (u32, u32);
    unsafe { asm!("rdmsr", in("ecx") 0x1Bu32, out("eax") lo, out("edx") hi, options(nostack)) };
    let base = ((hi as u64) << 32 | lo as u64) & 0xF_FFFF_F000;
    if lo & (1 << 11) == 0 || base == 0 || base >= 1 << 32 || !crate::nic::map_uc_4k(base) {
        crate::kprintln!("idle: no usable local APIC ({:#x}): busy polling", base);
        return false;
    }
    unsafe {
        BASE = base;
        IDLE_EOI = base + 0xB0;
        crate::port::outb(0x21, 0xFF); // mask both 8259s
        crate::port::outb(0xA1, 0xFF);
    }
    wr(0xF0, 0x100 | SPURIOUS as u32); // APIC software enable, spurious vector
    wr(0x320, 1 << 16 | VEC as u32); // timer: one-shot, masked
    wr(0x3E0, 0x3); // divide by 16
    wr(0x380, 0xFFFF_FFFF);
    crate::time::delay_us(10_000);
    let per_ms = (0xFFFF_FFFFu32 - rd(0x390)) / 10;
    wr(0x380, 0);
    let cs: u16;
    unsafe {
        asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack));
        let idt = &mut *core::ptr::addr_of_mut!(IDT);
        idt[VEC] = gate(idle_isr_timer, cs);
        idt[SPURIOUS] = gate(idle_isr_spurious, cs);
        let d = Idtr { limit: (core::mem::size_of::<[Gate; 256]>() - 1) as u16, base: idt.as_ptr() as u64 };
        asm!("lidt [{0}]", in(reg) &d, options(readonly, nostack));
        TICKS_PER_MS = per_ms;
    }
    crate::kprintln!("idle: APIC at {:#x}, timer {} ticks/ms: the CPU halts between polls", base, per_ms);
    per_ms > 0
}

/// Halt for up to `ms` (any interrupt ends it early). No-op if `init` failed.
pub fn nap(ms: u32) {
    let t = unsafe { TICKS_PER_MS };
    if t == 0 {
        return;
    }
    wr(0x320, VEC as u32); // unmasked one-shot
    wr(0x380, ms.saturating_mul(t).max(1));
    let t0 = crate::port::rdtsc();
    match unsafe { MWAIT } {
        None => unsafe { asm!("sti", "hlt", "cli", options(nomem, nostack)) },
        Some(h) => unsafe {
            asm!("monitor", in("rax") core::ptr::addr_of!(MON), in("ecx") 0u32, in("edx") 0u32, options(nostack));
            asm!("sti", "mwait", "cli", in("eax") h, in("ecx") 0u32, options(nostack)); // sti shadow covers mwait
        },
    }
    unsafe { SLEPT += crate::port::rdtsc().wrapping_sub(t0) };
    wr(0x320, 1 << 16 | VEC as u32);
    wr(0x380, 0);
}
