ENTRY(_start32)
SECTIONS {
    . = 0x100000;
    __load_start = .;
    .all : {
        KEEP(*(.multiboot_header))
        *(.text.boot)
        *(.text .text.*)
        *(.rodata .rodata.*)
        *(.data .data.*)
        *(.got .got.*)
    }
    __load_end = .;
    .bss (NOLOAD) : ALIGN(4096) {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4096);
        __bss_end = .;
    }
    /DISCARD/ : { *(.eh_frame*) *(.comment) }
}
