/* Multiboot v1 entry: 32-bit protected mode -> long mode -> kmain (Rust).
   AT&T syntax. Header uses the a.out kludge (flag bit 16) so QEMU -kernel,
   GRUB `multiboot` and iPXE all load this ELF64 file by raw addresses. */

.set MB_MAGIC, 0x1BADB002
.set MB_FLAGS, 0x00010003          /* page-align | mem info | address kludge. NOT video (bit 2): GRUB then sets a
                                      graphics mode even under gfxpayload=text, and the NC110 VBIOS offers no 1024x600
                                      (it picked 800x600: black screen, 2026-10-20). console.rs keeps the framebuffer
                                      renderer for a future direct GMA mode set. */
.set TCO_TICKS, 5                  /* 5 x 0.6 s = 3 s per stage; reset on 2nd expiry, about 6 s */

.section .multiboot_header, "a"
.align 4
mb_header:
    .long MB_MAGIC
    .long MB_FLAGS
    .long -(MB_MAGIC + MB_FLAGS)
    .long mb_header                 /* header_addr   */
    .long __load_start              /* load_addr     */
    .long __load_end                /* load_end_addr */
    .long __bss_end                 /* bss_end_addr  */
    .long _start32                  /* entry_addr    */

.section .text.boot, "ax"
.code32
.global _start32
_start32:
    cli
    cld
    mov %eax, %ebp                  /* multiboot magic */
    mov %ebx, %esi                  /* multiboot info ptr */

    /* ---- Arm the TCO watchdog FIRST (safety rule 5 rehearsal). No stack, no paging. ----
       Self-arming happens ONLY on LPC 00:1f.0 = ICH9 8086:2918 (QEMU q35). On NM10 8086:27bc (NC110)
       GRUB owns the watchdog: this code never writes TCO/GCS there, Rust only verifies it live.
       Sequence mirrors Linux iTCO_wdt v2: clear GCS.NO_REBOOT, clear SMI_EN.TCO_EN,
       clear stale TCO status, program TCO_TMR, reload, clear TCO1_CNT.TCO_TMR_HLT (RMW only). */
    mov $0x8000F800, %eax           /* bus 0, dev 31, fn 0, off 0x00: vendor/device */
    mov $0xCF8, %dx
    outl %eax, %dx
    mov $0xCFC, %dx
    inl %dx, %eax
    cmp $0x29188086, %eax
    je 4f
    cmp $0x27BC8086, %eax
    jne 1f
    movl $5, tco_result             /* 5 = NM10: GRUB armed it, hands off */
    jmp 9f
1:  movl $1, tco_result             /* 1 = skipped, no ICH9/NM10 LPC */
    jmp 9f
4:  cmpb $0, TCO_GRUB_ARMED
    je 3f
    movl $4, tco_result             /* 4 = left to GRUB (build feature grub-armed) */
    jmp 9f
3:  mov $0x8000F840, %eax           /* PMBASE (ABASE) */
    mov $0xCF8, %dx
    outl %eax, %dx
    mov $0xCFC, %dx
    inl %dx, %eax
    and $0xFF80, %eax
    jz 7f
    mov %eax, %ebx                  /* ebx = PMBASE */
    mov $0x8000F8F0, %eax           /* RCBA */
    mov $0xCF8, %dx
    outl %eax, %dx
    mov $0xCFC, %dx
    inl %dx, %eax
    test $1, %eax
    jz 8f
    and $0xFFFFC000, %eax
    mov %eax, %edi                  /* edi = RCBA (physical, paging is off) */
    mov 0x3410(%edi), %eax
    mov %eax, tco_gcs_pre
    cmpb $0, TCO_NEG_CONTROL
    jne 6f
    andl $0xFFFFFFDF, 0x3410(%edi)  /* GCS: clear NO_REBOOT (bit 5) */
    jmp 5f
6:  orl $0x20, 0x3410(%edi)         /* negative control: SET NO_REBOOT, as firmware leaves it */
5:  mov 0x3410(%edi), %eax
    mov %eax, tco_gcs
    lea 0x60(%ebx), %ecx            /* ecx = TCOBASE = PMBASE + 0x60 */
    mov %ecx, tco_base
    lea 4(%ecx), %edx
    inw %dx, %ax
    mov %ax, tco_sts1_pre
    lea 6(%ecx), %edx
    inw %dx, %ax
    mov %ax, tco_sts2_pre
    lea 0x30(%ebx), %edx            /* SMI_EN: clear TCO_EN (bit 13) so SMM can't eat timeout 1 */
    inl %dx, %eax
    and $0xFFFFDFFF, %eax
    outl %eax, %dx
    lea 4(%ecx), %edx
    mov $0x0008, %ax                /* TCO1_STS: clear TIMEOUT (RW1C) */
    outw %ax, %dx
    lea 6(%ecx), %edx
    mov $0x0002, %ax                /* TCO2_STS: clear SECOND_TO_STS first, then BOOT_STS (RW1C) */
    outw %ax, %dx
    mov $0x0004, %ax
    outw %ax, %dx
    lea 0x12(%ecx), %edx            /* TCO_TMR: keep 15:10, ticks of 0.6 s in 9:0 */
    inw %dx, %ax
    and $0xFC00, %ax
    or $TCO_TICKS, %ax
    outw %ax, %dx
    mov %ecx, %edx                  /* TCO_RLD: any write reloads */
    mov $1, %ax
    outw %ax, %dx
    lea 8(%ecx), %edx               /* TCO1_CNT: clear TCO_TMR_HLT (bit 11), RMW, never touch TCO_LOCK */
    inw %dx, %ax
    and $0xF7FF, %ax
    outw %ax, %dx
    inw %dx, %ax
    mov %ax, tco_cnt
    movl $0, tco_result             /* 0 = arm sequence ran (Rust verifies live) */
    jmp 9f
7:  movl $2, tco_result             /* 2 = PMBASE unset */
    jmp 9f
8:  movl $3, tco_result             /* 3 = RCBA disabled */
9:

    /* zero .bss (page tables + stack live there) */
    mov $__bss_start, %edi
    mov $__bss_end, %ecx
    sub %edi, %ecx
    shr $2, %ecx
    xor %eax, %eax
    rep stosl

    mov $stack_top, %esp

    /* PML4[0] -> PDPT ; PDPT[0..3] -> PD0..PD3 ; identity map 4 GiB, 2 MiB pages */
    mov $pdpt, %eax
    or $0x3, %eax
    mov %eax, pml4
    xor %ecx, %ecx
1:  mov %ecx, %eax
    shl $12, %eax
    add $pd, %eax
    or $0x3, %eax
    mov %eax, pdpt(,%ecx,8)
    inc %ecx
    cmp $4, %ecx
    jne 1b
    xor %ecx, %ecx
2:  mov %ecx, %eax
    shl $21, %eax
    or $0x83, %eax                  /* present | rw | 2MiB */
    mov %eax, pd(,%ecx,8)
    inc %ecx
    cmp $2048, %ecx
    jne 2b

    mov $pml4, %eax
    mov %eax, %cr3
    mov %cr4, %eax
    or $0x20, %eax                  /* PAE */
    mov %eax, %cr4
    mov $0xC0000080, %ecx           /* EFER */
    rdmsr
    or $0x100, %eax                 /* LME */
    wrmsr
    mov %cr0, %eax
    or $0x80000001, %eax            /* PG | PE */
    mov %eax, %cr0

    lgdt gdt_ptr
    ljmp $0x08, $start64

.code64
start64:
    mov $0x10, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %ss
    xor %ax, %ax
    mov %ax, %fs
    mov %ax, %gs
    mov $stack_top, %rsp
    mov %ebp, %edi                  /* arg0: magic */
    mov %esi, %esi                  /* arg1: info ptr (zero-extended) */
    call kmain
3:  cli
    hlt
    jmp 3b

.section .data
.align 4
.global tco_result, tco_gcs_pre, tco_gcs, tco_base, tco_sts1_pre, tco_sts2_pre, tco_cnt
tco_result:   .long 0xFFFFFFFF
tco_gcs_pre:  .long 0
tco_gcs:      .long 0
tco_base:     .long 0
tco_sts1_pre: .word 0
tco_sts2_pre: .word 0
tco_cnt:      .word 0

.section .rodata
.align 8
gdt:
    .quad 0
    .quad 0x00AF9A000000FFFF        /* 0x08: 64-bit code */
    .quad 0x00CF92000000FFFF        /* 0x10: data */
gdt_end:
gdt_ptr:
    .word gdt_end - gdt - 1
    .long gdt

.section .bss
.align 4096
pml4: .skip 4096
pdpt: .skip 4096
.global pd
pd:   .skip 4096 * 4
.align 16
stack_bottom: .skip 2097152
stack_top:

/* ---- In-RAM chain loader trampoline (bmctl `lcom` / `lcomlinux`). NOT executed here: loader.rs copies the bytes
   between tramp_start and tramp_end to a free low page and calls it as fn(params). Position-independent (RIP-relative),
   own GDT and stack inside that page. Order matters: the old kernel's page tables live in its .bss at 1 MiB, which
   the new image overwrites, so paging is switched OFF before a single byte is copied.
   params (u64 each, values < 4 GiB): +0 src, +8 dst, +16 len, +24 bss_len, +32 entry, +40 ebx (multiboot info or
   Linux boot_params), +48 mode (0 = multiboot v1: EAX=0x2BADB002 EBX=info; 1 = Linux 32-bit boot protocol:
   ESI=boot_params, EBP=EDI=EBX=0). GDT: 0x10 = flat 32-bit code (__BOOT_CS), 0x18 = flat data (__BOOT_DS), as the
   Linux 32-bit protocol requires; 0x08 = the kernel's 64-bit code so CS stays valid across lgdt. */
.section .rodata
.global tramp_start, tramp_end
.code64
tramp_start:
    cli
    cld
    lea tgdt(%rip), %rax
    mov %rax, tgdt_ptr+2(%rip)
    lgdt tgdt_ptr(%rip)
    mov 48(%rdi), %r8
    mov 0(%rdi), %rsi
    mov 16(%rdi), %rcx
    mov 24(%rdi), %rdx
    mov 32(%rdi), %rbp
    mov 40(%rdi), %rbx
    mov 8(%rdi), %rdi
    lea tstack_top(%rip), %rsp
    push %r8                       /* mode, popped (low dword) in 32-bit mode after the copy */
    lea t32(%rip), %rax
    pushq $0x10
    push %rax
    lretq
.code32
t32:
    mov $0x18, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %ss
    mov %ax, %fs
    mov %ax, %gs
    mov %cr0, %eax
    and $0x7FFFFFFF, %eax          /* paging off (we run identity-mapped, so execution continues) */
    mov %eax, %cr0
    push %ecx
    push %edx
    mov $0xC0000080, %ecx          /* EFER.LME off */
    rdmsr
    and $0xFFFFFEFF, %eax
    wrmsr
    pop %edx
    pop %ecx
    rep movsb                      /* image -> load address */
    mov %edx, %ecx
    xor %eax, %eax
    rep stosb                      /* zero bss (multiboot) */
    pop %eax                       /* mode */
    test %eax, %eax
    jnz 1f
    mov $0x2BADB002, %eax
    jmp *%ebp                      /* multiboot: ebx = info */
1:  mov %ebx, %esi                 /* Linux: esi = boot_params; ebp, edi, ebx must be 0 */
    mov %ebp, %ecx
    xor %eax, %eax
    xor %ebx, %ebx
    xor %edi, %edi
    xor %ebp, %ebp
    jmp *%ecx
.align 8
tgdt:
    .quad 0
    .quad 0x00AF9A000000FFFF        /* 0x08: 64-bit code (same as the kernel's) */
    .quad 0x00CF9A000000FFFF        /* 0x10: 32-bit code */
    .quad 0x00CF92000000FFFF        /* 0x18: 32-bit data */
tgdt_ptr:
    .word 4 * 8 - 1
    .quad 0
.align 16
    .skip 256
tstack_top:
tramp_end:
.code64 /* back to 64-bit for whatever global_asm the compiler places after this file (idle.rs ISR) */
