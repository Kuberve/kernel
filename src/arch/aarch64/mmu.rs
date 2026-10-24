use core::arch::asm;
use crate::arch::aarch64::phys;

#[repr(C, align(4096))]
pub struct PageTable {
    entries: [u64; 512],
}

#[allow(dead_code)]
pub const PAGE_SIZE: usize = 4096;

#[derive(Clone, Copy)]
pub struct AddressSpace {
    pub l0_pa: u64,
    pub l1_pa: u64,
    pub l2_pa: u64,
    pub l2_dev_pa: u64,
    pub asid: u16,
    pub valid: bool,
}

pub const EMPTY_AS: AddressSpace = AddressSpace {
    l0_pa: 0, l1_pa: 0, l2_pa: 0, l2_dev_pa: 0, asid: 0, valid: false,
};

const MAIR: u64 = 0xFF | (0x00 << 8);

const TCR: u64 = (16 << 0)
    | (1 << 8) | (1 << 10) | (3 << 12) | (0 << 14)
    | (16 << 16) | (1 << 24) | (1 << 26) | (3 << 28) | (2 << 30)
    | (2 << 32)
    | (1 << 36);   // AS=1: ASID in TTBR0[63:48]

pub const BLOCK: u64 = 0b01;
pub const TABLE: u64 = 0b11;
pub const AF: u64 = 1 << 10;
pub const SH_INNER: u64 = 3 << 8;
pub const ATTR_NORMAL: u64 = 0 << 2;
pub const ATTR_DEVICE: u64 = 1 << 2;
pub const PXN: u64 = 1 << 53;
pub const UXN: u64 = 1 << 54;
pub const AP_RW_ALL: u64 = 1 << 6;

/// Глобальная инициализация: MAIR, TCR. TTBR0 ставится через activate().
pub unsafe fn init() {
    asm!("msr mair_el1, {}", in(reg) MAIR);
    asm!("msr tcr_el1, {}", in(reg) TCR);
    asm!("dsb sy");
    asm!("isb");
}

/// Включить MMU (SCTLR.M, C, I).
pub unsafe fn enable() {
    let mut sctlr: u64;
    asm!("mrs {}, sctlr_el1", out(reg) sctlr);
    sctlr |= 1;
    sctlr |= 1 << 2;
    sctlr |= 1 << 12;
    sctlr &= !(1 << 19);
    asm!("msr sctlr_el1, {}", in(reg) sctlr);
    asm!("isb");
}

/// Создать новое адресное пространство.
pub unsafe fn create_as(thread_id: usize, asid: u16) -> AddressSpace {
    let l0_pa = phys::alloc_page().expect("no page for L0");
    let l1_pa = phys::alloc_page().expect("no page for L1");
    let l2_pa = phys::alloc_page().expect("no page for L2");
    let l2_dev_pa = phys::alloc_page().expect("no page for L2_DEV");

    let l0 = &mut *(l0_pa as *mut PageTable);
    let l1 = &mut *(l1_pa as *mut PageTable);
    let l2 = &mut *(l2_pa as *mut PageTable);
    let l2_dev = &mut *(l2_dev_pa as *mut PageTable);

    l0.entries = [0; 512];
    l1.entries = [0; 512];
    l2.entries = [0; 512];
    l2_dev.entries = [0; 512];

    l0.entries[0] = l1_pa as u64 | TABLE;
    l1.entries[0] = l2_dev_pa as u64 | TABLE;
    l1.entries[1] = l2_pa as u64 | TABLE;

    l1.entries[256] = 0x4000000000u64 | ATTR_DEVICE | AF | PXN | UXN | BLOCK;

    for i in 0..512 {
        let pa = (i as u64) * 0x200000;
        l2_dev.entries[i] = pa | ATTR_DEVICE | AF | PXN | UXN | BLOCK;
    }
    l2_dev.entries[72] = 0x09000000u64
        | ATTR_DEVICE | AF | AP_RW_ALL | PXN | UXN | BLOCK;

    l2.entries[0] = 0x40000000u64 | ATTR_NORMAL | AF | SH_INNER | BLOCK;
    l2.entries[1] = 0x40200000u64 | ATTR_NORMAL | AF | SH_INNER | UXN | PXN | BLOCK;

    if asid == 0 {
        // === Boot AS: kernel работает в identity, phys::alloc_page() использует
        //     VA = PA. Оставляем l2[2..5] identity, kernel-only, не исполняемо из EL0.
        l2.entries[2] = 0x40400000u64 | ATTR_NORMAL | AF | SH_INNER | UXN | PXN | BLOCK;
        l2.entries[3] = 0x40600000u64 | ATTR_NORMAL | AF | SH_INNER | UXN | PXN | BLOCK;
        l2.entries[4] = 0x40800000u64 | ATTR_NORMAL | AF | SH_INNER | UXN | PXN | BLOCK;
    } else {
        // === User AS: изоляция. PA раскладка (проверена на отсутствие пересечений):
        //     phys alloc:  0x403bc000..0x46000000
        //     stack PA:    0x46000000..0x48000000   (16 × 2MB)
        //     framebuffer: 0x48000000..0x48200000
        //     code  PA:    0x49000000..0x4B000000   (16 × 2MB)
        //     heap  PA:    0x4D000000..0x4F000000   (16 × 2MB)
        let code_pa  = 0x49000000u64 + (thread_id as u64) * 0x200000;
        let stack_pa = 0x46000000u64 + (thread_id as u64) * 0x200000;
        let heap_pa  = 0x4D000000u64 + (thread_id as u64) * 0x200000;

        l2.entries[2] = code_pa  | ATTR_NORMAL | AF | SH_INNER | AP_RW_ALL | PXN        | BLOCK;  // user exec
        l2.entries[3] = stack_pa | ATTR_NORMAL | AF | SH_INNER | AP_RW_ALL | UXN | PXN  | BLOCK;  // stack
        l2.entries[4] = heap_pa  | ATTR_NORMAL | AF | SH_INNER | AP_RW_ALL | UXN | PXN  | BLOCK;  // heap
    }

    for i in 5..512 {
        let pa = 0x40000000u64 + (i as u64) * 0x200000;
        l2.entries[i] = pa | ATTR_NORMAL | AF | SH_INNER | UXN | PXN | BLOCK;
    }

    AddressSpace {
        l0_pa: l0_pa as u64,
        l1_pa: l1_pa as u64,
        l2_pa: l2_pa as u64,
        l2_dev_pa: l2_dev_pa as u64,
        asid,
        valid: true,
    }
}

/// Активировать AS. ASID в TTBR0[63:48], PA в [47:0].
///
/// Без tlbi: TLB тегируется по ASID, записи разных AS невидимы друг другу.
/// TCR_EL1.AS=1 включает эту схему; A1=0 => ASID в TTBR0.
pub unsafe fn activate(as_: &AddressSpace) {
    let ttbr0_val = (as_.l0_pa & 0x0000_FFFF_FFFF_FFFF) | ((as_.asid as u64) << 48);
    asm!("msr ttbr0_el1, {}", in(reg) ttbr0_val);
    asm!("dsb sy");
    asm!("isb");
}