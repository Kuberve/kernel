.balign 2048
.global exception_vector_table
exception_vector_table:
    /* --- Group 1: Exceptions from the current EL using SP_EL0 --- */
    .balign 128
    b . // Synchronouns
    .balign 128
    b . // IRQ
    .balign 128
    b . // FIQ
    .balign 128
    b . // SError

    /* --- Group 2: Exceptions from the current EL using SP_ELx (Kernel) --- */
    .balign 128
    b exc_sync_el1  // Synchronous (memory errors, svc, etc.)
    .balign 128
    b exc_irq_el1   // Hardware interrupt (IRQ)
    .balign 128
    b . // FIQ
    .balign 128
    b . // SError

    /* --- Group 3: Exceptions from lower EL64 (from 64‑bit User‑space) --- */
    .balign 128
    b . // Synchronous (example, Syscall)
    .balign 128
    b . // IRQ
    .balign 128
    b . // FIQ
    .balign 128
    b . // SError

    /* --- Group 4: Exceptions from the lower EL32 (32‑bit mode) --- */
    .balign 128
    b .
    .balign 128
    b .
    .balign 128
    b .
    .balign 128
    b .

/* --------------------------------------------------------------
    Macro for saving and restoring context (Processing template)    
   -------------------------------------------------------------- */
.macro kernel_entry
    sub     sp, sp, #288  // Выделяем больше места (30 регистров + 2 системных)
    stp     x0, x1, [sp, #16 * 0]
    stp     x2, x3, [sp, #16 * 1]
    stp     x4, x5, [sp, #16 * 2]
    stp     x6, x7, [sp, #16 * 3]
    stp     x8, x9, [sp, #16 * 4]
    stp     x10, x11, [sp, #16 * 5]
    stp     x12, x13, [sp, #16 * 6]
    stp     x14, x15, [sp, #16 * 7]
    stp     x16, x17, [sp, #16 * 8]
    stp     x18, x19, [sp, #16 * 9]
    stp     x20, x21, [sp, #16 * 10]
    stp     x22, x23, [sp, #16 * 11]
    stp     x24, x25, [sp, #16 * 12]
    stp     x26, x27, [sp, #16 * 13]
    stp     x28, x29, [sp, #16 * 14]
    
    // Обязательно сохраняем адрес возврата и статус процессора!
    mrs     x22, elr_el1
    mrs     x23, spsr_el1
    stp     x30, x22, [sp, #16 * 15]
    str     x23, [sp, #16 * 16]
.endm

.macro kernel_exit
    // Восстанавливаем статус процессора и адрес возврата
    ldr     x23, [sp, #16 * 16]
    ldp     x30, x22, [sp, #16 * 15]
    msr     elr_el1, x22
    msr     spsr_el1, x23

    ldp     x28, x29, [sp, #16 * 14]
    ldp     x26, x27, [sp, #16 * 13]
    ldp     x24, x25, [sp, #16 * 12]
    ldp     x22, x23, [sp, #16 * 11]
    ldp     x20, x21, [sp, #16 * 10]
    ldp     x18, x19, [sp, #16 * 9]
    ldp     x16, x17, [sp, #16 * 8]
    ldp     x14, x15, [sp, #16 * 7]
    ldp     x12, x13, [sp, #16 * 6]
    ldp     x10, x11, [sp, #16 * 5]
    ldp     x8, x9, [sp, #16 * 4]
    ldp     x6, x7, [sp, #16 * 3]
    ldp     x4, x5, [sp, #16 * 2]
    ldp     x2, x3, [sp, #16 * 1]
    ldp     x0, x1, [sp, #16 * 0]
    add     sp, sp, #288
    eret
.endm

/* Implementation of specific handlers (jump to C) */
exc_sync_el1:
    kernel_entry
    bl      c_sync_handler
    kernel_exit

exc_irq_el1:
    kernel_entry
    bl      c_irq_handler
    kernel_exit