const GICD_BASE: usize = 0x0800_0000;
const GICR_BASE: usize = 0x080A_0000;

pub fn init() {
    unsafe {
        // Включаем system register interface
        // SRE=1, DFB=1, DIB=1
        let sre: u64 = (1 << 0) | (1 << 1) | (1 << 2);
        core::arch::asm!("msr ICC_SRE_EL1, {}", in(reg) sre);
        core::arch::asm!("isb");

        // Разрешаем все приоритеты
        core::arch::asm!("msr ICC_PMR_EL1, {}", in(reg) 0xFFu64);

        // Включаем Group 1
        core::arch::asm!("msr ICC_IGRPEN1_EL1, {}", in(reg) 1u64);
        core::arch::asm!("isb");

        // GICD: включаем Group 1
        let gicd_ctlr = (GICD_BASE + 0x0000) as *mut u32;
        // ARE_NS (bit 4) + EnableGrp1NS (bit 1)
        core::ptr::write_volatile(gicd_ctlr, (1 << 4) | (1 << 1));

        // Дождаться завершения записи (RWP = bit 31)
        while core::ptr::read_volatile(gicd_ctlr) & (1 << 31) != 0 {}

        // GICR: первый redistributor (у нас одно ядро)
        // GICR_WAKER @ 0x0014
        let waker = (GICR_BASE + 0x0014) as *mut u32;
        let mut v = core::ptr::read_volatile(waker);
        v &= !(1 << 1); // clear ProcessorSleep
        core::ptr::write_volatile(waker, v);
        // Ждём, пока ChildrenAsleep сбросится
        while core::ptr::read_volatile(waker) & (1 << 2) != 0 {}

        // SGI_base = GICR + 0x10000
        let sgi = GICR_BASE + 0x10000;

        // PPI 30 — в Group 1
        let igroupr0 = (sgi + 0x0080) as *mut u32;
        let g = core::ptr::read_volatile(igroupr0);
        core::ptr::write_volatile(igroupr0, g | (1 << 30));

        // Enable PPI 30
        let isenabler0 = (sgi + 0x0100) as *mut u32;
        core::ptr::write_volatile(isenabler0, 1 << 30);
    }
}