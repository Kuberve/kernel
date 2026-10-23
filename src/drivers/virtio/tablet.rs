use crate::println;
use crate::arch::aarch64::phys;
use crate::drivers::pci::{config_read32, config_write32};

// MMIO helpers
unsafe fn read_u8(b: u64, o: u64) -> u8 { core::ptr::read_volatile((b+o) as *const u8) }
unsafe fn read_u16(b: u64, o: u64) -> u16 { core::ptr::read_volatile((b+o) as *const u16) }
unsafe fn read_u32(b: u64, o: u64) -> u32 { core::ptr::read_volatile((b+o) as *const u32) }
unsafe fn write_u8(b: u64, o: u64, v: u8) { core::ptr::write_volatile((b+o) as *mut u8, v) }
unsafe fn write_u16(b: u64, o: u64, v: u16) { core::ptr::write_volatile((b+o) as *mut u16, v) }
unsafe fn write_u32(b: u64, o: u64, v: u32) { core::ptr::write_volatile((b+o) as *mut u32, v) }

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VirtqDesc { pub addr: u64, pub len: u32, pub flags: u16, pub next: u16 }
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VirtqUsedElem { pub id: u32, pub len: u32 }

#[repr(C, align(4096))]
pub struct AvailPage { pub flags: u16, pub idx: u16, pub ring: [u16; 256], pub used_event: u16 }
#[repr(C, align(4096))]
pub struct UsedPage { pub flags: u16, pub idx: u16, pub ring: [VirtqUsedElem; 256], pub avail_event: u16 }

pub struct Virtqueue {
    pub desc_pa: u64,
    pub avail_pa: u64,
    pub used_pa: u64,
    pub size: u16,
    pub next_avail: u16,
    pub last_used_idx: u16,
}

pub struct VirtioTablet {
    pub bar: u64,
    pub notify_off: u32,
    pub notify_mult: u32,
    pub event_q: Virtqueue,
    pub buf_pa: u64,
    pub abs_max_x: u32,
    pub abs_max_y: u32,
}

const NUM_EVENTS: usize = 256;

pub unsafe fn init() -> Option<VirtioTablet> {
    let (bus, dev, func) = crate::drivers::pci::find_virtio_tablet()?;

    let bar4_lo = config_read32(bus, dev, func, 0x20);
    let bar4_hi = config_read32(bus, dev, func, 0x24);
    let bar_type = (bar4_lo >> 1) & 3;
    let bar_pa = if (bar4_lo & !0xF) == 0 && bar4_hi == 0 {
        let target: u64 = 0x10400000;
        let new_lo = (target as u32) | 0x4;
        config_write32(bus, dev, func, 0x20, new_lo);
        config_write32(bus, dev, func, 0x24, (target >> 32) as u32);
        println!("virtio-tablet: BAR4 assigned at {:#x}", target);
        target
    } else if bar_type == 2 {
        ((bar4_hi as u64) << 32) | ((bar4_lo as u64) & !0xF)
    } else {
        (bar4_lo as u64) & !0xF
    };
    println!("virtio-tablet: BAR4 (final) = {:#x}", bar_pa);

    // caps
    let cap_ptr = (config_read32(bus, dev, func, 0x34) & 0xFF) as u8;
    let mut notify_off = 0u32;
    let mut notify_mult = 4u32;
    let mut device_cap_off = 0u32;

    let mut ptr = cap_ptr;
    let mut count = 0;
    while ptr != 0 && count < 20 {
        let cap_dw0 = config_read32(bus, dev, func, ptr as u16);
        let cap_dw1 = config_read32(bus, dev, func, (ptr + 4) as u16);
        let cap_id = cap_dw0 & 0xFF;
        let next = ((cap_dw0 >> 8) & 0xFF) as u8;
        if cap_id == 0x09 {
            let cap_dw2 = config_read32(bus, dev, func, (ptr + 8) as u16);
            let cfg_type = ((cap_dw0 >> 24) & 0xFF) as u8;
            if cfg_type == 2 {
                notify_off = cap_dw2;
                notify_mult = config_read32(bus, dev, func, (ptr + 16) as u16);
            }
            if cfg_type == 4 {
                device_cap_off = cap_dw2;
            }
        }
        ptr = next;
        count += 1;
    }

    // status
    let s0 = read_u8(bar_pa, 0x14);
    let _ = s0;

    // features
    write_u32(bar_pa, 0x00, 0);
    let feat_lo = read_u32(bar_pa, 0x04);
    write_u32(bar_pa, 0x00, 1);
    let feat_hi = read_u32(bar_pa, 0x04);
    let dev_features = ((feat_hi as u64) << 32) | (feat_lo as u64);

    write_u8(bar_pa, 0x14, 1);
    write_u8(bar_pa, 0x14, 1 | 2);

    let want: u64 = 1u64 << 32;
    if (want & !dev_features) != 0 {
        println!("virtio-tablet: VERSION_1 not supported");
        return None;
    }

    write_u32(bar_pa, 0x08, 0);
    write_u32(bar_pa, 0x0C, (want & 0xFFFFFFFF) as u32);
    write_u32(bar_pa, 0x08, 1);
    write_u32(bar_pa, 0x0C, (want >> 32) as u32);

    write_u8(bar_pa, 0x14, 1 | 2 | 8);
    let s1 = read_u8(bar_pa, 0x14);
    if (s1 & 8) == 0 { println!("virtio-tablet: FEATURES_OK rejected"); return None; }

    // === Read abs_max from device config ===
    // virtio_input_config: select(1) subsel(1) size(1) reserved(5) data[128]
    // select=1 (ABS_INFO), subsel=0 (ABS_X), subsel=1 (ABS_Y)
    // data at offset +8: { min(4), max(4), fuzz(4), flat(4), res(4) } = absinfo
    let mut abs_max_x = 32767u32;
    let mut abs_max_y = 32767u32;

    if device_cap_off != 0 {
        // select = 1 (ABS_INFO), subsel = 0 (ABS_X)
        write_u8(bar_pa, device_cap_off as u64, 1);
        write_u8(bar_pa, device_cap_off as u64 + 1, 0);
        let max_x = read_u32(bar_pa, device_cap_off as u64 + 8 + 4);
        // subsel = 1 (ABS_Y)
        write_u8(bar_pa, device_cap_off as u64 + 1, 1);
        let max_y = read_u32(bar_pa, device_cap_off as u64 + 8 + 4);
        if max_x > 0 && max_y > 0 {
            abs_max_x = max_x;
            abs_max_y = max_y;
        }
    }
    println!("virtio-tablet: abs_max_x={} abs_max_y={}", abs_max_x, abs_max_y);

    // === Setup event queue ===
    write_u16(bar_pa, 0x16, 0);
    let q0_size = read_u16(bar_pa, 0x18);
    if q0_size == 0 { return None; }

    let desc_pa = phys::alloc_page()?;
    let avail_pa = phys::alloc_page()?;
    let used_pa = phys::alloc_page()?;
    let buf_pa = phys::alloc_page()?;
    core::ptr::write_bytes(desc_pa as *mut u8, 0, 4096);
    core::ptr::write_bytes(avail_pa as *mut u8, 0, 4096);
    core::ptr::write_bytes(used_pa as *mut u8, 0, 4096);
    core::ptr::write_bytes(buf_pa as *mut u8, 0, 4096);

    let n = core::cmp::min(q0_size as usize, NUM_EVENTS);
    for i in 0..n {
        let d = &mut *((desc_pa + i * 16) as *mut VirtqDesc);
        d.addr = (buf_pa + i * 8) as u64;
        d.len = 8;
        d.flags = 2;
        d.next = 0;
    }

    let avail = &mut *(avail_pa as *mut AvailPage);
    for i in 0..n {
        avail.ring[i] = i as u16;
    }
    avail.idx = n as u16;

    write_u16(bar_pa, 0x16, 0);
    write_u16(bar_pa, 0x18, q0_size);
    write_u32(bar_pa, 0x20, desc_pa as u32);
    write_u32(bar_pa, 0x24, (desc_pa >> 32) as u32);
    write_u32(bar_pa, 0x28, avail_pa as u32);
    write_u32(bar_pa, 0x2C, (avail_pa >> 32) as u32);
    write_u32(bar_pa, 0x30, used_pa as u32);
    write_u32(bar_pa, 0x34, (used_pa >> 32) as u32);
    write_u16(bar_pa, 0x1C, 1);

    // notify
    write_u16(bar_pa, 0x16, 0);
    let noff = read_u16(bar_pa, 0x1E);
    let addr = bar_pa + notify_off as u64 + (noff as u64) * notify_mult as u64;
    core::arch::asm!("dsb sy");
    write_u16(addr, 0, 0);

    // status queue (q1) — пустая
    write_u16(bar_pa, 0x16, 1);
    let q1_size = read_u16(bar_pa, 0x18);
    if q1_size > 0 {
        let sd = phys::alloc_page()?;
        let sa = phys::alloc_page()?;
        let su = phys::alloc_page()?;
        core::ptr::write_bytes(sd as *mut u8, 0, 4096);
        core::ptr::write_bytes(sa as *mut u8, 0, 4096);
        core::ptr::write_bytes(su as *mut u8, 0, 4096);
        write_u16(bar_pa, 0x16, 1);
        write_u16(bar_pa, 0x18, q1_size);
        write_u32(bar_pa, 0x20, sd as u32);
        write_u32(bar_pa, 0x24, (sd >> 32) as u32);
        write_u32(bar_pa, 0x28, sa as u32);
        write_u32(bar_pa, 0x2C, (sa >> 32) as u32);
        write_u32(bar_pa, 0x30, su as u32);
        write_u32(bar_pa, 0x34, (su >> 32) as u32);
        write_u16(bar_pa, 0x1C, 1);
    }

    let s = read_u8(bar_pa, 0x14);
    write_u8(bar_pa, 0x14, s | 4);
    let sf = read_u8(bar_pa, 0x14);
    println!("virtio-tablet: status={:#x}", sf);
    if (sf & 0x80) != 0 { println!("virtio-tablet: FAILED"); return None; }

    Some(VirtioTablet {
        bar: bar_pa,
        notify_off, notify_mult,
        event_q: Virtqueue {
            desc_pa: desc_pa as u64,
            avail_pa: avail_pa as u64,
            used_pa: used_pa as u64,
            size: q0_size,
            next_avail: n as u16,
            last_used_idx: 0,
        },
        buf_pa: buf_pa as u64,
        abs_max_x, abs_max_y,
    })
}

pub struct MouseEvent {
    pub typ: u16,
    pub code: u16,
    pub value: u32,
}

pub unsafe fn poll_events(tablet: &mut VirtioTablet) -> Option<MouseEvent> {
    let q = &mut tablet.event_q;
    let size = q.size as usize;
    let used = &mut *(q.used_pa as *mut UsedPage);
    let avail = &mut *(q.avail_pa as *mut AvailPage);

    core::arch::asm!("dsb sy");
    if q.last_used_idx == used.idx { return None; }

    let idx = (q.last_used_idx as usize) % size;
    let elem = used.ring[idx];
    let desc_id = elem.id as usize;
    q.last_used_idx = q.last_used_idx.wrapping_add(1);

    let buf = (tablet.buf_pa + (desc_id * 8) as u64) as *const u8;
    let typ = u16::from_le_bytes([*buf, *buf.add(1)]);
    let code = u16::from_le_bytes([*buf.add(2), *buf.add(3)]);
    let value = u32::from_le_bytes([*buf.add(4), *buf.add(5), *buf.add(6), *buf.add(7)]);

    avail.ring[(q.next_avail as usize) % size] = desc_id as u16;
    q.next_avail = q.next_avail.wrapping_add(1);
    avail.idx = q.next_avail;
    core::arch::asm!("dsb sy");

    let noff = read_u16(tablet.bar, 0x1E);
    let addr = tablet.bar + tablet.notify_off as u64 + (noff as u64) * tablet.notify_mult as u64;
    write_u16(addr, 0, 0);

    Some(MouseEvent { typ, code, value })
}