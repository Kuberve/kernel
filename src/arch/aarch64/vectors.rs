use core::arch::{asm, global_asm};
use crate::println;

global_asm!(
    ".section .text.vectors",
    ".align 11",
    ".global _vectors",
    "_vectors:",

    ".balign 0x80", "b sync_handler",
    ".balign 0x80", "b irq_handler",
    ".balign 0x80", "b fiq_handler",
    ".balign 0x80", "b serr_handler",

    ".balign 0x80", "b sync_handler",
    ".balign 0x80", "b irq_handler",
    ".balign 0x80", "b fiq_handler",
    ".balign 0x80", "b serr_handler",

    ".balign 0x80", "b sync_handler",
    ".balign 0x80", "b irq_handler",
    ".balign 0x80", "b fiq_handler",
    ".balign 0x80", "b serr_handler",

    ".balign 0x80", "b sync_handler",
    ".balign 0x80", "b irq_handler",
    ".balign 0x80", "b fiq_handler",
    ".balign 0x80", "b serr_handler",

    "sync_handler:",
    "fiq_handler:",
    "serr_handler:",
    "  b rust_exception_handler",

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

    "  mrs x0, ELR_EL1",
    "  mrs x1, SPSR_EL1",
    "  stp x0, x1, [sp, #-16]!",

    "  mov x0, sp",
    "  bl rust_irq_handler",
    "  mov sp, x0",

    "  ldp x0, x1, [sp], #16",
    "  msr ELR_EL1, x0",
    "  msr SPSR_EL1, x1",

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
        let v = &_vectors as *const _ as u64;
        asm!("msr VBAR_EL1, {}", in(reg) v);
        asm!("isb");
    }
}

#[no_mangle]
pub extern "C" fn rust_exception_handler() -> ! {
    let esr: u64; unsafe { asm!("mrs {}, ESR_EL1", out(reg) esr) };
    let elr: u64; unsafe { asm!("mrs {}, ELR_EL1", out(reg) elr) };
    let far: u64; unsafe { asm!("mrs {}, FAR_EL1", out(reg) far) };
    let spsr: u64; unsafe { asm!("mrs {}, SPSR_EL1", out(reg) spsr) };

    println!();
    println!("!! EXCEPTION !!");
    println!("  EC       = {:#x}", (esr >> 26) & 0x3F);
    println!("  ESR_EL1  = {:#x}", esr);
    println!("  ELR_EL1  = {:#x}", elr);
    println!("  FAR_EL1  = {:#x}", far);
    println!("  SPSR_EL1 = {:#x}", spsr);
    println!("Halted.");
    loop { unsafe { asm!("wfe") } }
}

#[no_mangle]
pub unsafe extern "C" fn rust_irq_handler(old_sp: u64) -> u64 {
    let iar: u64;
    asm!("mrs {}, ICC_IAR1_EL1", out(reg) iar);
    let intid = (iar & 0xFFFFFF) as u32;

    if intid >= 1020 {
        return old_sp;
    }

    match intid {
        30 => crate::arch::aarch64::timer::handle_irq(),
        _ => {}
    }

    asm!("msr ICC_EOIR1_EL1, {}", in(reg) iar);

    crate::arch::aarch64::sched::on_tick(old_sp)
}