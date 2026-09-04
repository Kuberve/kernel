/* Kuberve Bootloader for ARMv8-A (AArch64) */
/* Загружается как ELF ядром или через UEFI */

.section .text.boot
.global _start

_start:
    /* Проверка, что мы на CPU0 (только один CPU при старте) */
    mrs x1, mpidr_el1
    and x1, x1, #0xFF
    cbnz x1, hang

    /* Установка стека (нижние 16KB зарезервированы под stack) */
    ldr x1, =_stack_top
    mov sp, x1

    /* Очистка BSS */
    ldr x1, =__bss_start
    ldr x2, =__bss_end
    bss_loop:
        cmp x1, x2
        b.hs bss_done
        str xzr, [x1], #8
        b bss_loop
    bss_done:

    /* Переход к ядру на C */
    bl kernel_main

    /* Если вернулось — зависаем */
hang:
    wfi
    b hang

.section .bss
.align 12
_stack_start:
    .skip 16384
_stack_top: