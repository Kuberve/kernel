use core::arch::asm;
use crate::println;
use crate::arch::aarch64::phys;

// Register offsets (from BAR0)
const REG_CAP:   usize = 0x00;
const REG_VS:    usize = 0x08;
const REG_INTMS: usize = 0x0C;
const REG_CC:    usize = 0x14;
const REG_CSTS:  usize = 0x1C;
const REG_AQA:   usize = 0x24;
const REG_ASQ:   usize = 0x28;
const REG_ACQ:   usize = 0x30;
const REG_DBS:   usize = 0x1000;

// CC bits
const CC_EN:      u32 = 1 << 0;
const CC_IOSQES:  u32 = 6 << 16;  // 2^6 = 64 bytes
const CC_IOCQES:  u32 = 4 << 20;  // 2^4 = 16 bytes

// CSTS bits
const CSTS_RDY: u32 = 1 << 0;
const CSTS_CFS: u32 = 1 << 1;

fn read32(bar: u64, off: usize) -> u32 {
    unsafe { core::ptr::read_volatile((bar + off as u64) as *const u32) }
}

fn write32(bar: u64, off: usize, v: u32) {
    unsafe { core::ptr::write_volatile((bar + off as u64) as *mut u32, v) }
}

fn read64(bar: u64, off: usize) -> u64 {
    let lo = read32(bar, off) as u64;
    let hi = read32(bar, off + 4) as u64;
    (hi << 32) | lo
}

fn write64(bar: u64, off: usize, v: u64) {
    write32(bar, off, v as u32);
    write32(bar, off + 4, (v >> 32) as u32);
}

/// Admin queue size (в записях). 64 записи — с запасом.
const AQ_SIZE: u32 = 64;

/// SQ entry — 64 байта (16 dword)
/// CQ entry — 16 байт (4 dword)
static mut ASQ: *mut u8 = core::ptr::null_mut();
static mut ACQ: *mut u8 = core::ptr::null_mut();

pub struct Nvme {
    pub bar: u64,
    pub sq_head: u32,
    pub sq_tail: u32,
    pub cq_head: u32,
    pub cq_phase: bool,
    pub cid: u16,
}

pub unsafe fn init(bar: u64) -> Option<Nvme> {
    // 1. Проверить CAP (уже прочитали в main.rs, но на всякий случай)
    let cap = read64(bar, REG_CAP);
    let mqes = (cap & 0xFFFF) as u32;
    let dstrd = ((cap >> 32) & 0xF) as u32;
    println!("NVMe: MQES={} DSTRD={}", mqes, dstrd);

    // 2. Reset: CC.EN=0, подождать CSTS.RDY=0
    write32(bar, REG_CC, 0);
    let mut t = 0u64;
    while read32(bar, REG_CSTS) & CSTS_RDY != 0 {
        t += 1;
        if t > 100_000_000 { println!("NVMe: reset timeout"); return None; }
    }

    // 3. Выделить ASQ и ACQ (нужны физически непрерывные страницы)
    let asq_pa = phys::alloc_page()?;
    let acq_pa = phys::alloc_page()?;
    // Обнулить
    core::ptr::write_bytes(asq_pa as *mut u8, 0, 4096);
    core::ptr::write_bytes(acq_pa as *mut u8, 0, 4096);

    ASQ = asq_pa as *mut u8;
    ACQ = acq_pa as *mut u8;

    // 4. AQA: ASQS-1 в битах 0-11, ACQS-1 в битах 16-27
    let aqa = ((AQ_SIZE - 1) << 16) | (AQ_SIZE - 1);
    write32(bar, REG_AQA, aqa);

    // 5. ASQ/ACQ base
    write64(bar, REG_ASQ, asq_pa as u64);
    write64(bar, REG_ACQ, acq_pa as u64);
    println!("NVMe: ASQ={:#x} ACQ={:#x} AQA={:#x}", asq_pa, acq_pa, aqa);

    // 6. Включить контроллер
    let cc = CC_EN | CC_IOSQES | CC_IOCQES;
    write32(bar, REG_CC, cc);
    println!("NVMe: CC = {:#x}", cc);

    // 7. Подождать CSTS.RDY=1
    t = 0;
    while read32(bar, REG_CSTS) & CSTS_RDY == 0 {
        t += 1;
        if t > 100_000_000 {
            println!("NVMe: enable timeout, CSTS={:#x}", read32(bar, REG_CSTS));
            return None;
        }
    }
    println!("NVMe: controller ready, CSTS={:#x}", read32(bar, REG_CSTS));

    Some(Nvme {
        bar,
        sq_head: 0,
        sq_tail: 0,
        cq_head: 0,
        cq_phase: true,
        cid: 0,
    })
}