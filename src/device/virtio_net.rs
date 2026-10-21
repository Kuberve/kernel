use crate::println;
use crate::device::pci::{config_read32, config_write32, bar_size_mask, assign_bar64};
use crate::arch::aarch64::phys;

// ============================================================
// MMIO helpers (доступ к BAR4 регионам)
// ============================================================

unsafe fn read_u8(base: u64, off: u64) -> u8 {
    core::ptr::read_volatile((base + off) as *const u8)
}
unsafe fn read_u16(base: u64, off: u64) -> u16 {
    core::ptr::read_volatile((base + off) as *const u16)
}
unsafe fn read_u32(base: u64, off: u64) -> u32 {
    core::ptr::read_volatile((base + off) as *const u32)
}
unsafe fn write_u8(base: u64, off: u64, v: u8) {
    core::ptr::write_volatile((base + off) as *mut u8, v);
}
unsafe fn write_u16(base: u64, off: u64, v: u16) {
    core::ptr::write_volatile((base + off) as *mut u16, v);
}
unsafe fn write_u32(base: u64, off: u64, v: u32) {
    core::ptr::write_volatile((base + off) as *mut u32, v);
}

// ============================================================
// Virtqueue structures (Virtio 1.0 §2.6.6)
// ============================================================

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VirtqDesc {
    pub addr: u64,   // физический адрес буфера
    pub len: u32,
    pub flags: u16,  // NEXT=1, WRITE=2, INDIRECT=4
    pub next: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VirtqUsedElem {
    pub id: u32,
    pub len: u32,
}

// Каждая структура в отдельной 4KB странице
#[repr(C, align(4096))]
pub struct DescPage {
    pub descs: [VirtqDesc; 256],
}

#[repr(C, align(4096))]
pub struct AvailPage {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; 256],
    pub used_event: u16,
}

#[repr(C, align(4096))]
pub struct UsedPage {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VirtqUsedElem; 256],
    pub avail_event: u16,
}

pub struct Virtqueue {
    pub desc_pa: u64,
    pub avail_pa: u64,
    pub used_pa: u64,
    pub size: u16,
    pub last_used_idx: u16,
    pub next_avail: u16,
}

pub struct VirtioNet {
    pub bar: u64,
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub mac: [u8; 6],
    pub rx: Virtqueue,
    pub tx: Virtqueue,
    pub notify_off: u32,          // смещение NOTIFY cap в BAR4
    pub notify_mult: u32,         // notify_off_multiplier
}

// ============================================================
// RX setup and polling
// ============================================================

const RX_BUF_SIZE: usize = 2048;   // ≥ 1526 (Ethernet max frame) + 10 virtio_net_hdr
const NUM_RX_BUFS: usize = 256;

static mut RX_BUF_PTRS: [u64; NUM_RX_BUFS] = [0; NUM_RX_BUFS];

/// Заполнить RX queue дескрипторами + avail ring + notify device.
pub unsafe fn setup_rx(vn: &VirtioNet) -> bool {
    let desc_base = vn.rx.desc_pa;
    let avail_base = vn.rx.avail_pa;
    let size = vn.rx.size as usize;

    // 1. Заполнить descriptors
    for i in 0..size {
        let buf_pa = match phys::alloc_page() {
            Some(p) => p,
            None => { println!("virtio-net: RX alloc failed at {}", i); return false; }
        };
        core::ptr::write_bytes(buf_pa as *mut u8, 0, 4096);
        RX_BUF_PTRS[i] = buf_pa as u64;

        let desc = &mut *((desc_base + (i * 16) as u64) as *mut VirtqDesc);
        desc.addr = buf_pa as u64;
        desc.len = RX_BUF_SIZE as u32;
        desc.flags = 2;
        desc.next = 0;
    }

    // 2. Заполнить avail ring: 0, 1, 2, ..., size-1
    let avail = &mut *(avail_base as *mut AvailPage);
    avail.flags = 0;
    for i in 0..size {
        avail.ring[i] = i as u16;
    }
    avail.idx = size as u16;
    let avail = &*(avail_base as *const AvailPage);
    let used = &*(vn.rx.used_pa as *const UsedPage);

    // 3. Memory barrier
    core::arch::asm!("dsb sy");

    // 4. Notify device: RX queue idx = 0
    notify_queue(vn, 0);

    println!("virtio-net: RX queue filled ({} descriptors)", size);
    true
}

/// Прочитать notify_off для очереди qidx и записать doorbell.
unsafe fn notify_queue(vn: &VirtioNet, qidx: u16) {
    write_u16(vn.bar, 0x16, qidx);
    let notify_off = read_u16(vn.bar, 0x1E);
    let addr = vn.bar + vn.notify_off as u64 + (notify_off as u64) * vn.notify_mult as u64;
    core::arch::asm!("dsb sy");
    write_u16(addr, 0, qidx);
}

/// Поллинг RX: забрать все доступные пакеты, вывести EtherType.
pub unsafe fn poll_rx(vn: &mut VirtioNet) -> u32 {
    let avail_base = vn.rx.avail_pa;
    let used_base = vn.rx.used_pa;
    let size = vn.rx.size as usize;

    let used = &mut *(used_base as *mut UsedPage);
    let avail = &mut *(avail_base as *mut AvailPage);

    core::arch::asm!("dsb sy");

    let mut count = 0u32;
    while vn.rx.last_used_idx != used.idx {
        let idx = (vn.rx.last_used_idx as usize) % size;
        let elem = used.ring[idx];
        let desc_id = elem.id as usize;
        let pkt_len = elem.len as usize;

        if desc_id < size && pkt_len > 12 {
            let buf_pa = RX_BUF_PTRS[desc_id];
            let buf = core::slice::from_raw_parts(buf_pa as *const u8, pkt_len);
            // Modern virtio: 12-byte virtio_net_hdr перед фреймом
            let frame = &buf[12..];
            if frame.len() >= 14 {
                let dst_mac = &frame[0..6];
                let src_mac = &frame[6..12];
                let ethertype = ((frame[12] as u16) << 8) | frame[13] as u16;

                println!("RX pkt #{}: len={} (frame {} bytes) ethertype={:#06x}",
                         desc_id, pkt_len, pkt_len - 12, ethertype);
                println!("  dst={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                         dst_mac[0], dst_mac[1], dst_mac[2], dst_mac[3], dst_mac[4], dst_mac[5]);
                println!("  src={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                         src_mac[0], src_mac[1], src_mac[2], src_mac[3], src_mac[4], src_mac[5]);

                // ARP reply? кэшируем
                if ethertype == 0x0806 {
                    if let Some((spa, sha, op)) = crate::device::arp::parse_arp(frame) {
                        if op == 2 {   // reply
                            crate::device::arp::cache_insert(spa, sha);
                        }
                    }
                }
            }
        }

        // Вернуть desc в avail ring
        avail.ring[(avail.idx as usize) % size] = desc_id as u16;
        avail.idx = avail.idx.wrapping_add(1);
        vn.rx.last_used_idx = vn.rx.last_used_idx.wrapping_add(1);
        count += 1;

        if count >= size as u32 { break; }  // safety
    }

    if count > 0 {
        core::arch::asm!("dsb sy");
        notify_queue(vn, 0);   // снова сказать device "можешь писать"
    }
    count
}

// ============================================================
// TX setup and send
// ============================================================

static mut TX_BUF: [u8; 2048] = [0; 2048];
static mut TX_DESC_IDX: u16 = 0;

/// Отправить Ethernet-фрейм (без virtio_net_hdr — QEMU без MRG_RXBUF и без VIRTIO_NET_F_CSUM).
pub unsafe fn send_packet(vn: &mut VirtioNet, data: &[u8]) -> bool {
    if data.len() > 1500 { return false; }

    let desc_base = vn.tx.desc_pa;
    let avail_base = vn.tx.avail_pa;
    let used_base = vn.tx.used_pa;
    let size = vn.tx.size as usize;

    // Modern virtio: 12-byte virtio_net_hdr (всегда, даже без MRG_RXBUF)
    const HDR_SIZE: usize = 12;
    let buf_pa = &raw mut TX_BUF as *mut u8;

    // Zero header
    for i in 0..HDR_SIZE {
        core::ptr::write_volatile(buf_pa.add(i), 0);
    }
    // Frame после header
    for i in 0..data.len() {
        core::ptr::write_volatile(buf_pa.add(HDR_SIZE + i), data[i]);
    }

    let total_len = HDR_SIZE + data.len();

    // Заполнить desc[TX_DESC_IDX % size]
    let desc_idx = (vn.tx.next_avail as usize) % size;
    let desc = &mut *((desc_base + (desc_idx * 16) as u64) as *mut VirtqDesc);
    desc.addr = buf_pa as u64;
    desc.len = total_len as u32;
    desc.flags = 0;   // не WRITE (device читает из buf)
    desc.next = 0;

    // Добавить в avail ring
    let avail = &mut *(avail_base as *mut AvailPage);
    avail.ring[(vn.tx.next_avail as usize) % size] = desc_idx as u16;
    vn.tx.next_avail = vn.tx.next_avail.wrapping_add(1);
    avail.idx = vn.tx.next_avail;

    // Memory barrier
    core::arch::asm!("dsb sy");

    // Notify TX (queue 1)
    notify_queue(vn, 1);

    // Polling used ring TX — ждать пока device прочитает
    let used = &mut *(used_base as *mut UsedPage);
    let mut timeout = 0u64;
    while vn.tx.last_used_idx == used.idx {
        timeout += 1;
        if timeout > 10_000_000 {
            println!("virtio-net: TX timeout (used.idx={} last={})", used.idx, vn.tx.last_used_idx);
            return false;
        }
        core::arch::asm!("yield", options(nostack, nomem));
    }
    vn.tx.last_used_idx = used.idx;

    true
}

pub unsafe fn init() -> Option<VirtioNet> {
    let (bus, dev, func) = crate::device::pci::find_virtio_net()?;

    // Modern virtio: MMIO в BAR4 (reg 0x20), 64-bit
    let bar4_lo = config_read32(bus, dev, func, 0x20);
    let bar4_hi = config_read32(bus, dev, func, 0x24);
    println!("virtio-net: BAR4 lo={:#x} hi={:#x}", bar4_lo, bar4_hi);

    // BAR4 может быть 64-bit (bit 2 = 1) или 32-bit
    let bar_type = (bar4_lo >> 1) & 3;
    let bar_pa = if (bar4_lo & !0xF) == 0 && (bar4_hi == 0) {
        // Не назначен — назначаем на 0x10100000
        let target: u64 = 0x10100000;
        let new_lo = (target as u32) | 0x4;  // 64-bit MMIO
        let new_hi = (target >> 32) as u32;
        config_write32(bus, dev, func, 0x20, new_lo);
        config_write32(bus, dev, func, 0x24, new_hi);
        println!("virtio-net: BAR4 assigned at {:#x}", target);
        target
    } else if bar_type == 2 {
        // Уже назначен, 64-bit
        ((bar4_hi as u64) << 32) | ((bar4_lo as u64) & !0xF)
    } else {
        // 32-bit
        (bar4_lo as u64) & !0xF
    };

    println!("virtio-net: BAR4 (final) = {:#x}", bar_pa);

    // Читаем PCI capabilities для virtio
    let cap_ptr = (config_read32(bus, dev, func, 0x34) & 0xFF) as u8;
    println!("virtio-net: cap_ptr={:#x}", cap_ptr);

    if cap_ptr == 0 {
        println!("virtio-net: no capabilities (legacy mode?)");
    } else {
        let mut ptr = cap_ptr;
        let mut count = 0;
        while ptr != 0 && count < 20 {
            let cap_dw0 = config_read32(bus, dev, func, ptr as u16);
            let cap_dw1 = config_read32(bus, dev, func, (ptr + 4) as u16);
            let cap_id = cap_dw0 & 0xFF;
            let next = ((cap_dw0 >> 8) & 0xFF) as u8;
            let cap_len = ((cap_dw0 >> 16) & 0xFF) as u8;

            if cap_id == 0x09 {
                // Virtio PCI cap (Virtio 1.0 §4.1.4)
                // dw0: cap_vndr | (cap_next << 8) | (cap_len << 16) | (cfg_type << 24)
                // dw1: bar | padding[3]
                // dw2: offset
                // dw3: length
                let cap_dw2 = config_read32(bus, dev, func, (ptr + 8) as u16);
                let cap_dw3 = config_read32(bus, dev, func, (ptr + 12) as u16);

                let cfg_type = ((cap_dw0 >> 24) & 0xFF) as u8;
                let bar_idx = (cap_dw1 & 0xFF) as u8;
                let offset = cap_dw2;
                let length = cap_dw3;

                let cfg_type_name = match cfg_type {
                    1 => "COMMON",
                    2 => "NOTIFY",
                    3 => "ISR",
                    4 => "DEVICE",
                    _ => "?",
                };
                println!("virtio cap: type={} ({}) bar={} offset={:#x} length={:#x}",
                         cfg_type, cfg_type_name, bar_idx, offset, length);

                if cfg_type == 2 {
                    let mult = config_read32(bus, dev, func, (ptr + 16) as u16);
                    println!("  notify_off_multiplier = {}", mult);
                }
            }
            ptr = next;
            count += 1;
        }
    }

    // --- Readback BAR4 (проверка) ---
    let rb_lo = config_read32(bus, dev, func, 0x20);
    let rb_hi = config_read32(bus, dev, func, 0x24);
    println!("virtio-net: BAR4 readback = {:#x}{:08x}", rb_hi, rb_lo);

    // --- Helpers для MMIO ---
    unsafe fn read_u8(base: u64, off: u64) -> u8 {
        core::ptr::read_volatile((base + off) as *const u8)
    }
    unsafe fn read_u16(base: u64, off: u64) -> u16 {
        core::ptr::read_volatile((base + off) as *const u16)
    }
    unsafe fn read_u32(base: u64, off: u64) -> u32 {
        core::ptr::read_volatile((base + off) as *const u32)
    }
    unsafe fn write_u8(base: u64, off: u64, v: u8) {
        core::ptr::write_volatile((base + off) as *mut u8, v);
    }
    unsafe fn write_u16(base: u64, off: u64, v: u16) {
        core::ptr::write_volatile((base + off) as *mut u16, v);
    }
    unsafe fn write_u32(base: u64, off: u64, v: u32) {
        core::ptr::write_volatile((base + off) as *mut u32, v);
    }

    // --- Читаем начальное состояние ---
    let status0 = read_u8(bar_pa, 0x14);
    println!("virtio-net: device_status = {:#x} (expect 0x0)", status0);

    let num_queues = read_u16(bar_pa, 0x12);
    println!("virtio-net: num_queues = {} (need 2: RX=0, TX=1)", num_queues);

    // --- Читаем device features (64-bit) ---
    write_u32(bar_pa, 0x00, 0);
    let feat_lo = read_u32(bar_pa, 0x04);
    write_u32(bar_pa, 0x00, 1);
    let feat_hi = read_u32(bar_pa, 0x04);
    let dev_features = ((feat_hi as u64) << 32) | (feat_lo as u64);
    println!("virtio-net: device_features = {:#018x}", dev_features);

    // --- Feature negotiation ---
    // Status биты (Virtio 1.0 §2.1.1):
    //   ACKNOWLEDGE = 1, DRIVER = 2, DRIVER_OK = 4, FEATURES_OK = 8, NEEDS_RESET = 64, FAILED = 128

    // 1. ACKNOWLEDGE
    write_u8(bar_pa, 0x14, 1);
    // 2. DRIVER
    write_u8(bar_pa, 0x14, 1 | 2);

    // 3. Хотим: F_MAC(5), F_STATUS(16), F_VERSION_1(32)
    // Не включаем MRG_RXBUF(15) — усложняет RX (mergeable buffers)
    let want_features: u64 = (1u64 << 5) | (1u64 << 16) | (1u64 << 32);

    // Проверка: хотим только то, что поддерживает device
    if (want_features & !dev_features) != 0 {
        println!("virtio-net: ERROR: device doesn't support requested features");
        println!("  want  = {:#018x}", want_features);
        println!("  missing = {:#018x}", want_features & !dev_features);
        return None;
    }

    // Записать driver_feature
    write_u32(bar_pa, 0x08, 0);  // select lower
    write_u32(bar_pa, 0x0C, (want_features & 0xFFFFFFFF) as u32);
    write_u32(bar_pa, 0x08, 1);  // select upper
    write_u32(bar_pa, 0x0C, (want_features >> 32) as u32);
    println!("virtio-net: wrote driver_features = {:#018x}", want_features);

    // 4. FEATURES_OK
    write_u8(bar_pa, 0x14, 1 | 2 | 8);

    // 5. Readback: bit 8 должен остаться (QEMU принял features)
    let status1 = read_u8(bar_pa, 0x14);
    println!("virtio-net: status after FEATURES_OK = {:#x} (expect 0xb)", status1);
    if (status1 & 8) == 0 {
        println!("virtio-net: ERROR: FEATURES_OK rejected — negotiation failed");
        return None;
    }

    // --- Читаем размеры очередей ---
    // queue_select = offset 0x16, queue_size = offset 0x18
    write_u16(bar_pa, 0x16, 0);
    let q0_size = read_u16(bar_pa, 0x18);
    println!("virtio-net: queue[0] (RX) size = {}", q0_size);

    write_u16(bar_pa, 0x16, 1);
    let q1_size = read_u16(bar_pa, 0x18);
    println!("virtio-net: queue[1] (TX) size = {}", q1_size);

    if q0_size == 0 || q1_size == 0 {
        println!("virtio-net: ERROR: queue size 0, abort");
        return None;
    }

    // --- Setup virtqueues ---
    unsafe fn setup_queue(
        bar_pa: u64,
        qidx: u16,
        size: u16,
        write_u16: unsafe fn(u64, u64, u16),
        write_u32: unsafe fn(u64, u64, u32),
        read_u16: unsafe fn(u64, u64) -> u16,
    ) -> Option<Virtqueue> {
        // Выделить 3 страницы
        let desc_pa = phys::alloc_page()?;
        let avail_pa = phys::alloc_page()?;
        let used_pa = phys::alloc_page()?;

        // Обнулить
        core::ptr::write_bytes(desc_pa as *mut u8, 0, 4096);
        core::ptr::write_bytes(avail_pa as *mut u8, 0, 4096);
        core::ptr::write_bytes(used_pa as *mut u8, 0, 4096);

        // queue_select
        write_u16(bar_pa, 0x16, qidx);
        // queue_size (нужно записать то, что хотим; обычно device default ок,
        // но по spec надо записать 0..=queue_size, device clamp'ит)
        write_u16(bar_pa, 0x18, size);
        // queue_desc (offset 0x20, u64)
        write_u32(bar_pa, 0x20, desc_pa as u32);
        write_u32(bar_pa, 0x24, (desc_pa >> 32) as u32);
        // queue_driver = avail ring (offset 0x28, u64)
        write_u32(bar_pa, 0x28, avail_pa as u32);
        write_u32(bar_pa, 0x2C, (avail_pa >> 32) as u32);
        // queue_device = used ring (offset 0x30, u64)
        write_u32(bar_pa, 0x30, used_pa as u32);
        write_u32(bar_pa, 0x34, (used_pa >> 32) as u32);
        // queue_enable (offset 0x1C, u16) = 1
        write_u16(bar_pa, 0x1C, 1);

        // Readback
        let rb_size = read_u16(bar_pa, 0x18);
        println!("virtio-net: q{} enabled, size={} desc={:#x} avail={:#x} used={:#x}",
                 qidx, rb_size, desc_pa, avail_pa, used_pa);

        Some(Virtqueue {
            desc_pa: desc_pa as u64,
            avail_pa: avail_pa as u64,
            used_pa: used_pa as u64,
            size: rb_size,
            last_used_idx: 0,
            next_avail: 0,
        })
    }

    let rx_q = match setup_queue(bar_pa, 0, q0_size, write_u16, write_u32, read_u16) {
        Some(q) => q,
        None => { println!("virtio-net: setup RX failed"); return None; }
    };
    let tx_q = match setup_queue(bar_pa, 1, q1_size, write_u16, write_u32, read_u16) {
        Some(q) => q,
        None => { println!("virtio-net: setup TX failed"); return None; }
    };

    // --- DRIVER_OK ---
    let s = read_u8(bar_pa, 0x14);
    write_u8(bar_pa, 0x14, s | 4);
    let s_after = read_u8(bar_pa, 0x14);
    println!("virtio-net: status after DRIVER_OK = {:#x} (expect 0xf)", s_after);
    if (s_after & 0x80) != 0 {
        println!("virtio-net: DEVICE FAILED!");
        return None;
    }

    // --- Прочитать MAC из DEVICE cap ---
    // DEVICE cap offset 0x2000, virtio_net_config: mac[0..6]
    let mut mac = [0u8; 6];
    for i in 0..6 {
        mac[i] = read_u8(bar_pa, 0x2000 + i as u64);
    }
    println!("virtio-net: MAC = {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
             mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);

    // --- Link status (если F_STATUS) ---
    let link_status = read_u16(bar_pa, 0x2000 + 6);
    println!("virtio-net: link_status = {:#x} (1 = up)", link_status);

    Some(VirtioNet {
        bar: bar_pa,
        bus, dev, func,
        mac,
        rx: rx_q,
        tx: tx_q,
        notify_off: 0x3000,   // из cap NOTIFY offset
        notify_mult: 4,        // из notify_off_multiplier
    })
}