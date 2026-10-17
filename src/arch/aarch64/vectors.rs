use core::arch::global_asm;

global_asm!(
    ".section .text.vectors",
    ".align 11",
    ".global _vectors",
    "_vectors:",

    // Current EL with SP_EL0
    ".balign 0x80", "b sync_handler",
    ".balign 0x80", "b irq_handler",
    ".balign 0x80", "b fiq_handler",
    ".balign 0x80", "b serr_handler",

    // Current EL with SP_ELx
    ".balign 0x80", "b sync_handler",
    ".balign 0x80", "b irq_handler",
    ".balign 0x80", "b fiq_handler",
    ".balign 0x80", "b serr_handler",

    // Lower EL, AArch64
    ".balign 0x80", "b sync_handler",
    ".balign 0x80", "b irq_handler",
    ".balign 0x80", "b fiq_handler",
    ".balign 0x80", "b serr_handler",

    // Lower EL, AArch32
    ".balign 0x80", "b sync_handler",
    ".balign 0x80", "b irq_handler",
    ".balign 0x80", "b fiq_handler",
    ".balign 0x80", "b serr_handler",

    "sync_handler:",
    "fiq_handler:",
    "serr_handler:",
    "  b rust_panic_handler",

    "irq_handler:",
    "  msr daifset, #2",
    "  stp x0, x1, [sp, #-16]!",
    "  stp x2, x3, [sp, #-16]!",
    "  stp x4, x5, [sp, #-16]!",
    "  stp x6, x7, [sp, #-16]!",
    "  stp x8, x9, [sp, #-16]!",
    "  stp x10, x11, [sp, #-16]!",
    "  stp x12, x13, [sp, #-16]!",
    "  stp x14, x15, [sp, #-16]!",
    "  stp x16, x17, [sp, #-16]!",
    "  stp x18, x19, [sp, #-16]!",
    "  stp x20, x21, [sp, #-16]!",
    "  stp x22, x23, [sp, #-16]!",
    "  stp x24, x25, [sp, #-16]!",
    "  stp x26, x27, [sp, #-16]!",
    "  stp x28, x29, [sp, #-16]!",
    "  stp x30, xzr, [sp, #-16]!",

    "  bl rust_irq_handler",

    "  ldp x30, xzr, [sp], #16",
    "  ldp x28, x29, [sp], #16",
    "  ldp x26, x27, [sp], #16",
    "  ldp x24, x25, [sp], #16",
    "  ldp x22, x23, [sp], #16",
    "  ldp x20, x21, [sp], #16",
    "  ldp x18, x19, [sp], #16",
    "  ldp x16, x17, [sp], #16",
    "  ldp x14, x15, [sp], #16",
    "  ldp x12, x13, [sp], #16",
    "  ldp x10, x11, [sp], #16",
    "  ldp x8, x9, [sp], #16",
    "  ldp x6, x7, [sp], #16",
    "  ldp x4, x5, [sp], #16",
    "  ldp x2, x3, [sp], #16",
    "  ldp x0, x1, [sp], #16",

    "  msr daifclr, #2",
    "  eret",
);

extern "C" {
    static _vectors: u8;
}

pub fn init() {
    unsafe {
        let vbar = &_vectors as *const _ as u64;
        core::arch::asm!("msr vbar_el1, {}", in(reg) vbar);
        let back: u64;
        unsafe { core::arch::asm!("mrs {}, VBAR_EL1", out(reg) back) };
        crate::uart::puts("VBAR_EL1=");
        crate::uart::put_num(back);
        crate::uart::puts("\n");
        core::arch::asm!("isb");
    }
}

#[no_mangle]
pub extern "C" fn rust_panic_handler() -> ! {
    crate::uart::puts("\n!! EXCEPTION !!\n");

    let esr: u64;
    unsafe { core::arch::asm!("mrs {}, ESR_EL1", out(reg) esr) };
    crate::uart::puts("ESR_EL1=");
    crate::uart::put_num(esr);
    crate::uart::puts("\n");

    let elr: u64;
    unsafe { core::arch::asm!("mrs {}, ELR_EL1", out(reg) elr) };
    crate::uart::puts("ELR_EL1=");
    crate::uart::put_num(elr);
    crate::uart::puts("\n");

    let far: u64;
    unsafe { core::arch::asm!("mrs {}, FAR_EL1", out(reg) far) };
    crate::uart::puts("FAR_EL1=");
    crate::uart::put_num(far);
    crate::uart::puts("\n");

    let spsr: u64;
    unsafe { core::arch::asm!("mrs {}, SPSR_EL1", out(reg) spsr) };
    crate::uart::puts("SPSR_EL1=");
    crate::uart::put_num(spsr);
    crate::uart::puts("\n");

    loop { unsafe { core::arch::asm!("wfe") } }
}

#[no_mangle]
pub extern "C" fn rust_irq_handler() {
    // Acknowledge
    let iar: u64;
    unsafe { core::arch::asm!("mrs {}, ICC_IAR1_EL1", out(reg) iar) };
    let intid = (iar & 0xFFFFFF) as u32;

    if intid >= 1020 {
        // Spurious. НЕ пишем EOIR.
        return;
    }

    crate::uart::puts("irq id=");
    crate::uart::put_num(intid as u64);
    crate::uart::puts("\n");

    match intid {
        30 => crate::timer::tick(),        // PPI 30 — EL1 physical timer
        _ => {
            crate::uart::puts("irq ");
            // опционально: печатать intid
        }
    }

    // End of interrupt
    unsafe { core::arch::asm!("msr ICC_EOIR1_EL1, {}", in(reg) iar) };
}