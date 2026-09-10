/* Kuberve Kernel Linker Script — ARM64 */

ENTRY(_start)

SECTIONS {
    /* Ядро загружается по физическому адресу 4 МБ в DRAM */
    . = 0x40000000; 

    .text : {
        KEEP(*(.text.boot))
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .data : {
        *(.data .data.*)
    }

    .bss : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    . = ALIGN(16);
    . = . + 0x10000; 
    _stack_top = .;

    /DISCARD/ : {
        *(.comment)
        *(.gnu*)
        *(.note*)
        *(.eh_frame*)
    }

    _end = .;
}