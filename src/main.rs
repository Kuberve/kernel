#![no_std]
#![no_main]

use core::arch::{asm, global_asm};

mod arch;
use arch::{aarch64::gic, aarch64::vectors, aarch64::timer};

mod panic;
mod uart;

global_asm!(
    ".section .text.boot",
    ".global _start",
    "_start:",
    "  ldr x0, =_stack_top",
    "  mov sp, x0",
    "  bl kmain",
    "1:",
    "  wfe",
    "  b 1b",
);

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    uart::puts("Kuberve booting...\n");

    extern "C" { static _vectors: u8; }
    let vaddr = unsafe { &_vectors as *const _ as u64 };
    uart::puts("_vectors @ ");
    uart::put_num(vaddr);
    uart::puts("\n");

    vectors::init();
    let el: u64;
    unsafe { core::arch::asm!("mrs {}, CurrentEL", out(reg) el) };
    uart::puts("CurrentEL=");
    uart::put_num((el >> 2) & 3);
    uart::puts("\n");

    let frq: u64;
    unsafe { core::arch::asm!("mrs {}, CNTFRQ_EL0", out(reg) frq) };
    uart::puts("CNTFRQ_EL0=");
    uart::put_num(frq);
    uart::puts("\n");

    gic::init();

    timer::init();

    // DAIF
    let daif: u64;
    unsafe { core::arch::asm!("mrs {}, daif", out(reg) daif) };
    uart::puts("DAIF=");
    uart::put_num(daif);
    uart::puts("\n");

    // ICC_SRE_EL1 — должен быть 1 (SRE bit)
    let sre: u64;
    unsafe { core::arch::asm!("mrs {}, ICC_SRE_EL1", out(reg) sre) };
    uart::puts("ICC_SRE_EL1=");
    uart::put_num(sre);
    uart::puts("\n");

    // GICD_CTLR — ожидаем 0x12 (ARE_NS + EnableGrp1NS)
    let gicd = unsafe { core::ptr::read_volatile(0x0800_0000 as *const u32) };
    uart::puts("GICD_CTLR=");
    uart::put_num(gicd as u64);
    uart::puts("\n");

    // ICC_IGRPEN1_EL1 — должен быть 1
    let grpen: u64;
    unsafe { core::arch::asm!("mrs {}, ICC_IGRPEN1_EL1", out(reg) grpen) };
    uart::puts("ICC_IGRPEN1=");
    uart::put_num(grpen);
    uart::puts("\n");

    // ICC_PMR_EL1 — должен быть 0xFF (или меньше, но >= приоритета прерывания)
    let pmr: u64;
    unsafe { core::arch::asm!("mrs {}, ICC_PMR_EL1", out(reg) pmr) };
    uart::puts("ICC_PMR=");
    uart::put_num(pmr);
    uart::puts("\n");

    // CNTP_CTL_EL0 — ожидаем 1 (ENABLE, IMASK=0)
    let ctl: u64;
    unsafe { core::arch::asm!("mrs {}, CNTP_CTL_EL0", out(reg) ctl) };
    uart::puts("CNTP_CTL=");
    uart::put_num(ctl);
    uart::puts("\n");

    // CNTPCT_EL0 — счётчик, должен расти
    let ctr: u64;
    unsafe { core::arch::asm!("mrs {}, CNTPCT_EL0", out(reg) ctr) };
    uart::puts("CNTPCT=");
    uart::put_num(ctr);
    uart::puts("\n");

    // GICR_ISPENDR0 для PPI 30 — pending?
    let ispendr = unsafe {
        core::ptr::read_volatile((0x080A_0000 + 0x10000 + 0x0200) as *const u32)
    };
    uart::puts("GICR_ISPENDR0=");
    uart::put_num(ispendr as u64);
    uart::puts("\n");

    uart::puts("Enabling interrupts...\n");
    unsafe { asm!("msr daifclr, #2") };  // снять маску IRQ

    uart::puts("Kernel alive. Waiting for timer...\n");

    loop {
        unsafe { asm!("wfe") }
    }
}