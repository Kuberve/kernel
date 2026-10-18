use core::arch::asm;

/// 512 записей по 8 байт = 4KB, выровнено на 4KB.
#[repr(C, align(4096))]
struct PageTable {
    entries: [u64; 512],
}

static mut L0: PageTable = PageTable { entries: [0; 512] };
static mut L1: PageTable = PageTable { entries: [0; 512] };
static mut L2: PageTable = PageTable { entries: [0; 512] };

// MAIR: Attr0 = Normal WB (0xFF), Attr1 = Device nGnRnE (0x00)
const MAIR: u64 = 0xFF | (0x00 << 8);

// TCR для 48-bit VA, 4KB granule, WB cache
// T0SZ=16, IRGN0=WB, ORGN0=WB, SH0=inner, TG0=4K
// T1SZ=16, IRGN1=WB, ORGN1=WB, SH1=inner, TG1=4K
// IPS=1 (36-bit PA)
const TCR: u64 = (16 << 0)
    | (1 << 8)   // IRGN0
    | (1 << 10)  // ORGN0
    | (3 << 12)  // SH0
    | (0 << 14)  // TG0 = 4KB
    | (16 << 16) // T1SZ
    | (1 << 24)  // IRGN1
    | (1 << 26)  // ORGN1
    | (3 << 28)  // SH1
    | (2 << 30)  // TG1 = 4KB
    | (1 << 32); // IPS = 36-bit

// Descriptor bits для 1GB block на L1
const BLOCK: u64 = 0b01;
const AF: u64 = 1 << 10;
const SH_INNER: u64 = 3 << 8;
const ATTR_NORMAL: u64 = 0 << 2;
const ATTR_DEVICE: u64 = 1 << 2;
const PXN: u64 = 1 << 53;
const UXN: u64 = 1 << 54;

pub unsafe fn init() {
    let l0 = &mut *core::ptr::addr_of_mut!(L0);
    let l1 = &mut *core::ptr::addr_of_mut!(L1);

    // L0[0] -> L1 (table descriptor: lower bits 0b11)
    let l1_pa = l1 as *mut _ as u64;
    l0.entries[0] = l1_pa | 0b11;

        // L1[0] = 1GB device
    l1.entries[0] = 0x00000000u64
        | ATTR_DEVICE | AF | PXN | UXN | BLOCK;

    // L1[1] → L2
    let l2 = &mut *core::ptr::addr_of_mut!(L2);
    l1.entries[1] = l2 as *mut _ as u64 | 0b11;

    // Kernel code (L2[0]): AP=00 (EL1 only), exec allowed
    l2.entries[0] = 0x40000000u64
        | ATTR_NORMAL | AF | SH_INNER | BLOCK;

    // Kernel data/heap (L2[1]): AP=00, non-exec
    l2.entries[1] = 0x40200000u64
        | ATTR_NORMAL | AF | SH_INNER | UXN | PXN | BLOCK;

    // User code (L2[2]): AP=01 (EL1+EL0), exec only for EL0
    l2.entries[2] = 0x40400000u64
        | ATTR_NORMAL | AF | SH_INNER | (1 << 6) | PXN | BLOCK;

    // User data (L2[3]): AP=01, non-exec
    l2.entries[3] = 0x40600000u64
        | ATTR_NORMAL | AF | SH_INNER | (1 << 6) | UXN | PXN | BLOCK;

    // Rest (L2[4..512]): identity, non-exec, EL1 only
    for i in 4..512 {
        let pa = 0x40000000u64 + (i as u64) * 0x200000;
        l2.entries[i] = pa | ATTR_NORMAL | AF | SH_INNER | UXN | PXN | BLOCK;
    }

    // Настройка регистров MMU
    asm!("msr mair_el1, {}", in(reg) MAIR);
    asm!("msr tcr_el1, {}", in(reg) TCR);

    let l0_pa = l0 as *mut _ as u64;
    asm!("msr ttbr0_el1, {}", in(reg) l0_pa);
    asm!("msr ttbr1_el1, {}", in(reg) l0_pa);  // тот же набор — на всякий случай

    // Барьеры: гарантируем, что таблицы и регистры записаны до включения
    asm!("dsb sy");
    asm!("isb");

    // Включаем MMU: SCTLR_EL1.M = 1
    let mut sctlr: u64;
    asm!("mrs {}, sctlr_el1", out(reg) sctlr);
    sctlr |= 1;
    asm!("msr sctlr_el1, {}", in(reg) sctlr);
    asm!("isb");
}