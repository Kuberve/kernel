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
    "  msr daifset, #2",

    // SIMD (q0..q31) — первыми, самое высокое место на стеке
    "  stp q0, q1, [sp, #-32]!",
    "  stp q2, q3, [sp, #-32]!",
    "  stp q4, q5, [sp, #-32]!",
    "  stp q6, q7, [sp, #-32]!",
    "  stp q8, q9, [sp, #-32]!",
    "  stp q10, q11, [sp, #-32]!",
    "  stp q12, q13, [sp, #-32]!",
    "  stp q14, q15, [sp, #-32]!",
    "  stp q16, q17, [sp, #-32]!",
    "  stp q18, q19, [sp, #-32]!",
    "  stp q20, q21, [sp, #-32]!",
    "  stp q22, q23, [sp, #-32]!",
    "  stp q24, q25, [sp, #-32]!",
    "  stp q26, q27, [sp, #-32]!",
    "  stp q28, q29, [sp, #-32]!",
    "  stp q30, q31, [sp, #-32]!",

    // GPR (x0..x30) — сохраняются ПЕРЕД использованием x2/x3
    "  stp x0, x1, [sp, #-16]!",
    "  stp x2, x3, [sp, #-16]!",     // <- x2=len, x3=? сохраняются как есть
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

    // ELR/SPSR
    "  mrs x0, ELR_EL1",
    "  mrs x1, SPSR_EL1",
    "  stp x0, x1, [sp, #-16]!",

    "  mov x0, sp",
    "  bl rust_sync_handler",
    "  mov sp, x0",

    // Restore в обратном порядке
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

    "  ldp q30, q31, [sp], #32",
    "  ldp q28, q29, [sp], #32",
    "  ldp q26, q27, [sp], #32",
    "  ldp q24, q25, [sp], #32",
    "  ldp q22, q23, [sp], #32",
    "  ldp q20, q21, [sp], #32",
    "  ldp q18, q19, [sp], #32",
    "  ldp q16, q17, [sp], #32",
    "  ldp q14, q15, [sp], #32",
    "  ldp q12, q13, [sp], #32",
    "  ldp q10, q11, [sp], #32",
    "  ldp q8, q9, [sp], #32",
    "  ldp q6, q7, [sp], #32",
    "  ldp q4, q5, [sp], #32",
    "  ldp q2, q3, [sp], #32",
    "  ldp q0, q1, [sp], #32",

    "  msr daifclr, #2",
    "  eret",

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
pub unsafe extern "C" fn rust_sync_handler(old_sp: u64) -> u64 {
    use crate::arch::aarch64::sched::SavedCtx;
    use crate::arch::aarch64::{sched, syscall};

    let esr: u64;
    asm!("mrs {}, ESR_EL1", out(reg) esr);
    let ec = (esr >> 26) & 0x3F;

    if ec != 0x15 {
        crate::println!("!! sync exception EC={:#x} ESR={:#x}", ec, esr);
        loop { asm!("wfe"); }
    }

    let ctx = &mut *(old_sp as *mut SavedCtx);
    let num = ctx.x8;
    let a0  = ctx.x0;
    let a1  = ctx.x1;
    let a2  = ctx.x2;

    match num {
        syscall::SYS_YIELD => sched::on_yield(old_sp),
        syscall::SYS_WRITE => {
            let n = syscall::write(a0, a1 as *const u8, a2);
            ctx.x0 = n as u64;
            old_sp
        }
        syscall::SYS_SLEEP => {
            sched::sleep(a0);
            old_sp
        }
        syscall::SYS_GETPID => {
            ctx.x0 = sched::current_pid();
            old_sp
        }
        syscall::SYS_EXIT => sched::exit(),
        _ => {
            crate::println!("unknown syscall {}", num);
            ctx.x0 = u64::MAX;
            old_sp
        }
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