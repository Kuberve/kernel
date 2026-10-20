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

// ============================================================
// Admin command submission (SQ/CQ ring)
// ============================================================

pub unsafe fn submit_admin(nvme: &mut Nvme, cmd: &[u32; 16]) -> u32 {
    // 1. Записать SQE в SQ[nvme.sq_tail % AQ_SIZE]
    let sq = ASQ as *mut u32;
    let tail_idx = (nvme.sq_tail % AQ_SIZE) as usize;
    let entry = sq.add(tail_idx * 16);
    for i in 0..16 {
        core::ptr::write_volatile(entry.add(i), cmd[i]);
    }

    // 2. Ring SQ tail doorbell
    nvme.sq_tail = (nvme.sq_tail + 1) % AQ_SIZE;
    let dstrd = ((read64(nvme.bar, REG_CAP) >> 32) & 0xF) as usize;
    let sq_db_off = REG_DBS + (0 * 4 << dstrd);  // SQ 0
    let cq_db_off = REG_DBS + ((1 * 4) << dstrd); // CQ 0 (offset 4)
    write32(nvme.bar, sq_db_off, nvme.sq_tail);

    // 3. Poll CQ
    let cq = ACQ as *mut u32;
    loop {
        let head_idx = (nvme.cq_head % AQ_SIZE) as usize;
        let entry = cq.add(head_idx * 4);
        // Читаем статус (dword 3): phase bit в bits 16, status code в 17..31
        let status_dw = core::ptr::read_volatile(entry.add(3));
        let phase = ((status_dw >> 16) & 1) != 0;
        if phase != nvme.cq_phase {
            // Не готово, крутимся
            core::arch::asm!("yield", options(nostack, nomem));
            continue;
        }
        // Готово
        let status_code = (status_dw >> 17) & 0x7FFF;
        // Обновить CQ head
        nvme.cq_head = (nvme.cq_head + 1) % AQ_SIZE;
        if nvme.cq_head == 0 {
            nvme.cq_phase = !nvme.cq_phase;
        }
        // Ring CQ head doorbell
        write32(nvme.bar, cq_db_off, nvme.cq_head);

        return status_code;
    }
}

// ============================================================
// Identify
// ============================================================

pub struct IdentifyController {
    pub model: [u8; 40],
    pub serial: [u8; 20],
    pub firmware: [u8; 8],
    pub nn: u32,   // number of namespaces
}

pub struct IdentifyNamespace {
    pub nsze: u64,   // total blocks
    pub ncap: u64,
    pub lbads: u8,   // log2(block size)
}

pub unsafe fn identify_controller(nvme: &mut Nvme, buf_pa: u64) -> Option<IdentifyController> {
    let mut cmd = [0u32; 16];
    // dword 0: opcode=0x06, CID=1 (bits 16-31)
    cmd[0] = 0x06 | (1 << 16);
    // dword 1: NSID=0
    cmd[1] = 0;
    // dword 6-7: PRP1
    cmd[6] = buf_pa as u32;
    cmd[7] = (buf_pa >> 32) as u32;
    // dword 10: CDW10 = CNS 0x01 (identify controller)
    cmd[10] = 0x01;

    let status = submit_admin(nvme, &cmd);
    if status != 0 {
        println!("NVMe: identify controller status={:#x}", status);
        return None;
    }

    let data = buf_pa as *const u8;

    // Parse
    let mut model = [0u8; 40];
    let mut serial = [0u8; 20];
    let mut firmware = [0u8; 8];
    for i in 0..40 { model[i] = core::ptr::read_volatile(data.add(24 + i)); }
    for i in 0..20 { serial[i] = core::ptr::read_volatile(data.add(4 + i)); }
    for i in 0..8 { firmware[i] = core::ptr::read_volatile(data.add(64 + i)); }

    let nn = core::ptr::read_volatile(data.add(516) as *const u32);

    Some(IdentifyController { model, serial, firmware, nn })
}

pub unsafe fn identify_namespace(nvme: &mut Nvme, buf_pa: u64, nsid: u32) -> Option<IdentifyNamespace> {
    let mut cmd = [0u32; 16];
    cmd[0] = 0x06 | (2 << 16);  // CID=2
    cmd[1] = nsid;
    cmd[6] = buf_pa as u32;
    cmd[7] = (buf_pa >> 32) as u32;
    cmd[10] = 0x00;  // CNS = 0x00 (identify namespace)

    let status = submit_admin(nvme, &cmd);
    if status != 0 {
        println!("NVMe: identify namespace status={:#x}", status);
        return None;
    }

    let data = buf_pa as *const u8;
    let nsze = core::ptr::read_volatile(data as *const u64);
    let ncap = core::ptr::read_volatile(data.add(8) as *const u64);
    let lbads = core::ptr::read_volatile(data.add(128 + 2));

    Some(IdentifyNamespace { nsze, ncap, lbads })
}

// ============================================================
// I/O queues
// ============================================================

static mut IO_SQ: *mut u8 = core::ptr::null_mut();
static mut IO_CQ: *mut u8 = core::ptr::null_mut();

const IO_QID: u32 = 1;
const IO_Q_SIZE: u32 = 64;

/// Создать I/O CQ (opcode 0x05) и I/O SQ (opcode 0x01) через admin команды.
pub unsafe fn create_io_queues(nvme: &mut Nvme) -> bool {
    let io_cq_pa = match phys::alloc_page() {
        Some(p) => p,
        None => return false,
    };
    let io_sq_pa = match phys::alloc_page() {
        Some(p) => p,
        None => return false,
    };
    core::ptr::write_bytes(io_cq_pa as *mut u8, 0, 4096);
    core::ptr::write_bytes(io_sq_pa as *mut u8, 0, 4096);
    IO_CQ = io_cq_pa as *mut u8;
    IO_SQ = io_sq_pa as *mut u8;

    // --- Create I/O CQ ---
    let mut cmd = [0u32; 16];
    cmd[0] = 0x05 | (3 << 16);  // opcode=0x05, CID=3
    // dword 6-7: PRP1 = io_cq_pa
    cmd[6] = io_cq_pa as u32;
    cmd[7] = (io_cq_pa >> 32) as u32;
    // dword 10: QID[15:0], QSIZE-1 [31:16]
    cmd[10] = IO_QID | ((IO_Q_SIZE - 1) << 16);
    // dword 11: PC=1 (physically contiguous), IEN=0 (no IRQ)
    cmd[11] = 1;

    let s = submit_admin(nvme, &cmd);
    if s != 0 {
        println!("NVMe: create io cq status={:#x}", s);
        return false;
    }
    println!("NVMe: I/O CQ created (qid={}, size={})", IO_QID, IO_Q_SIZE);

    // --- Create I/O SQ ---
    let mut cmd = [0u32; 16];
    cmd[0] = 0x01 | (4 << 16);  // opcode=0x01, CID=4
    cmd[6] = io_sq_pa as u32;
    cmd[7] = (io_sq_pa >> 32) as u32;
    cmd[10] = IO_QID | ((IO_Q_SIZE - 1) << 16);
    // dword 11: PC=1, QPRIO=0, CQID=1 (bits 31:16)
    cmd[11] = 1 | (IO_QID << 16);

    let s = submit_admin(nvme, &cmd);
    if s != 0 {
        println!("NVMe: create io sq status={:#x}", s);
        return false;
    }
    println!("NVMe: I/O SQ created (qid={}, size={})", IO_QID, IO_Q_SIZE);

    true
}

// --- I/O ring state ---
static mut IO_SQ_TAIL: u32 = 0;
static mut IO_CQ_HEAD: u32 = 0;
static mut IO_CQ_PHASE: bool = true;

fn io_doorbell_sq(nvme: &Nvme) -> usize {
    let dstrd = ((read64(nvme.bar, REG_CAP) >> 32) & 0xF) as usize;
    REG_DBS + ((IO_QID as usize) * 2) * (4 << dstrd)
}

fn io_doorbell_cq(nvme: &Nvme) -> usize {
    let dstrd = ((read64(nvme.bar, REG_CAP) >> 32) & 0xF) as usize;
    REG_DBS + ((IO_QID as usize) * 2 + 1) * (4 << dstrd)
}

/// Отправить I/O команду, дождаться завершения. Возвращает status code.
pub unsafe fn submit_io(nvme: &Nvme, cmd: &[u32; 16]) -> u32 {
    let sq = IO_SQ as *mut u32;
    let tail_idx = (IO_SQ_TAIL % IO_Q_SIZE) as usize;
    let entry = sq.add(tail_idx * 16);
    for i in 0..16 {
        core::ptr::write_volatile(entry.add(i), cmd[i]);
    }

    IO_SQ_TAIL = (IO_SQ_TAIL + 1) % IO_Q_SIZE;
    write32(nvme.bar, io_doorbell_sq(nvme), IO_SQ_TAIL);

    // Poll CQ
    let cq = IO_CQ as *mut u32;
    loop {
        let head_idx = (IO_CQ_HEAD % IO_Q_SIZE) as usize;
        let entry = cq.add(head_idx * 4);
        let status_dw = core::ptr::read_volatile(entry.add(3));
        let phase = ((status_dw >> 16) & 1) != 0;
        if phase != IO_CQ_PHASE {
            core::arch::asm!("yield", options(nostack, nomem));
            continue;
        }
        let status_code = (status_dw >> 17) & 0x7FFF;
        IO_CQ_HEAD = (IO_CQ_HEAD + 1) % IO_Q_SIZE;
        if IO_CQ_HEAD == 0 {
            IO_CQ_PHASE = !IO_CQ_PHASE;
        }
        write32(nvme.bar, io_doorbell_cq(nvme), IO_CQ_HEAD);
        return status_code;
    }
}

// ============================================================
// Read / Write
// ============================================================

/// Прочитать `num_blocks` блоков с LBA `slba` в буфер `buf_pa`.
/// `buf_pa` должен быть выровнен на 512 байт (на 4KB страницу — идеально).
pub unsafe fn read_blocks(
    nvme: &Nvme,
    nsid: u32,
    slba: u64,
    num_blocks: u16,
    buf_pa: u64,
) -> bool {
    let mut cmd = [0u32; 16];
    cmd[0] = 0x02 | (10 << 16);  // opcode=0x02 Read, CID=10
    cmd[1] = nsid;
    // dword 6-7: PRP1
    cmd[6] = buf_pa as u32;
    cmd[7] = (buf_pa >> 32) as u32;
    // dword 10-11: SLBA (64-bit)
    cmd[10] = slba as u32;
    cmd[11] = (slba >> 32) as u32;
    // dword 12: NLB (bits 15:0) = num_blocks - 1
    cmd[12] = (num_blocks as u32 - 1) & 0xFFFF;

    let s = submit_io(nvme, &cmd);
    if s != 0 {
        println!("NVMe: read status={:#x}", s);
        return false;
    }
    true
}

/// Записать `num_blocks` блоков с LBA `slba` из буфера `buf_pa`.
pub unsafe fn write_blocks(
    nvme: &Nvme,
    nsid: u32,
    slba: u64,
    num_blocks: u16,
    buf_pa: u64,
) -> bool {
    let mut cmd = [0u32; 16];
    cmd[0] = 0x01 | (11 << 16);  // opcode=0x01 Write, CID=11
    cmd[1] = nsid;
    cmd[6] = buf_pa as u32;
    cmd[7] = (buf_pa >> 32) as u32;
    cmd[10] = slba as u32;
    cmd[11] = (slba >> 32) as u32;
    cmd[12] = (num_blocks as u32 - 1) & 0xFFFF;

    let s = submit_io(nvme, &cmd);
    if s != 0 {
        println!("NVMe: write status={:#x}", s);
        return false;
    }
    true
}