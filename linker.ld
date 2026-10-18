ENTRY(_start)

SECTIONS
{
    . = 0x40000000;

    .text : {
        KEEP(*(.text.boot))
        . = ALIGN(0x800);
        KEEP(*(.text.vectors))
        *(.text .text.*)
    }

    .rodata : { *(.rodata .rodata.*) }
    .data   : { *(.data .data.*) }
    .bss    : { *(.bss .bss.*) *(COMMON) }

    . = ALIGN(16);
    . += 0x10000;         /* 64 KiB стек */
    _stack_top = .;
    
    . = ALIGN(0x1000);
    __kernel_end = .;
}