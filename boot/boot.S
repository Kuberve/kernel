/* Kuberve Bootloader for ARMv8-A (AArch64) */
/* It is loaded as an ELF kernel or via UEFI. */

.section ".text.boot"
.global _start

_start:
    /* Save the x0 register! U-Boot passes a pointer to the Device Tree (DTB) in it. */
    /* We will really need this address in the C code to read the configuration. */
    mov     x19, x0 

    /* Switching from EL2 to EL1 */
    mrs     x0, CurrentEL
    and     x0, x0, #12
    cmp     x0, #8
    b.ne    .L_in_el1

    mov     x0, #(1 << 31) // RW=1 (64‑bit mode for EL1)
    msr     hcr_el2, x0

    mov     x0, #0x3c5     // We are masking interrupts, EL1h mode.
    msr     spsr_el2, x0

    adr     x0, .L_in_el1
    msr     elr_el2, x0
    eret

.L_in_el1:
    /* We initialize the stack pointer by taking the address from the linker script. */
    ldr     x0, =_stack_top
    mov     sp, x0

    /* We zero out the .bss section (so that global variables in C are equal to 0). */
    ldr     x0, =__bss_start
    ldr     x1, =__bss_end
.L_clear_bss:
    cmp     x0, x1
    b.ge    .L_bss_done
    str     xzr, [x0], #8
    b       .L_clear_bss

.L_bss_done:
    /* We pass the Device Tree address back as the first argument to the C function. */
    mov     x0, x19 

    /* We jump into the main C code of the operating system. */
    bl      kernel_main

    /* In case kernel_main returns control (which shouldn’t happen) */
1:  wfi
    b       1b