use crate::println;
use crate::arch::aarch64::phys;
use crate::drivers::pci::{config_read32, config_write32};

// ============================================================
// MMIO helpers (как virtio-net)
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
// Virtqueue structures (как в virtio-net)
// ============================================================
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VirtqUsedElem {
    pub id: u32,
    pub len: u32,
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
    pub next_avail: u16,
    pub last_used_idx: u16,
}

pub struct VirtioGpu {
    pub bar: u64,
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub dev_features: u64,
    pub control_q: Virtqueue,
    pub notify_off: u32,
    pub notify_mult: u32,
}

pub unsafe fn init() -> Option<VirtioGpu> {
    let (bus, dev, func) = crate::drivers::pci::find_virtio_gpu()?;

    // BAR4 — modern virtio
    let bar4_lo = config_read32(bus, dev, func, 0x20);
    let bar4_hi = config_read32(bus, dev, func, 0x24);
    println!("virtio-gpu: BAR4 lo={:#x} hi={:#x}", bar4_lo, bar4_hi);

    let bar_type = (bar4_lo >> 1) & 3;
    let bar_pa = if (bar4_lo & !0xF) == 0 && (bar4_hi == 0) {
        let target: u64 = 0x10200000;   // после virtio-net (0x10100000)
        let new_lo = (target as u32) | 0x4;
        config_write32(bus, dev, func, 0x20, new_lo);
        config_write32(bus, dev, func, 0x24, (target >> 32) as u32);
        println!("virtio-gpu: BAR4 assigned at {:#x}", target);
        target
    } else if bar_type == 2 {
        ((bar4_hi as u64) << 32) | ((bar4_lo as u64) & !0xF)
    } else {
        (bar4_lo as u64) & !0xF
    };
    println!("virtio-gpu: BAR4 (final) = {:#x}", bar_pa);

    // PCI caps
    let cap_ptr = (config_read32(bus, dev, func, 0x34) & 0xFF) as u8;
    println!("virtio-gpu: cap_ptr={:#x}", cap_ptr);

    let mut notify_off = 0u32;
    let mut notify_mult = 4u32;

    if cap_ptr != 0 {
        let mut ptr = cap_ptr;
        let mut count = 0;
        while ptr != 0 && count < 20 {
            let cap_dw0 = config_read32(bus, dev, func, ptr as u16);
            let cap_dw1 = config_read32(bus, dev, func, (ptr + 4) as u16);
            let cap_id = cap_dw0 & 0xFF;
            let next = ((cap_dw0 >> 8) & 0xFF) as u8;

            if cap_id == 0x09 {
                let cap_dw2 = config_read32(bus, dev, func, (ptr + 8) as u16);
                let cap_dw3 = config_read32(bus, dev, func, (ptr + 12) as u16);
                let cfg_type = ((cap_dw0 >> 24) & 0xFF) as u8;
                let bar_idx = (cap_dw1 & 0xFF) as u8;
                let offset = cap_dw2;
                let length = cap_dw3;
                let name = match cfg_type {
                    1 => "COMMON", 2 => "NOTIFY", 3 => "ISR", 4 => "DEVICE", _ => "?",
                };
                println!("virtio cap: type={} ({}) bar={} offset={:#x} length={:#x}",
                         cfg_type, name, bar_idx, offset, length);
                if cfg_type == 2 {
                    notify_off = offset;
                    notify_mult = config_read32(bus, dev, func, (ptr + 16) as u16);
                    println!("  notify_off={:#x} mult={}", notify_off, notify_mult);
                }
            }
            ptr = next;
            count += 1;
        }
    }

    // === Feature negotiation ===
    let s0 = read_u8(bar_pa, 0x14);
    println!("virtio-gpu: device_status={:#x}", s0);

    let num_queues = read_u16(bar_pa, 0x12);
    println!("virtio-gpu: num_queues={} (need 1: control)", num_queues);

    write_u32(bar_pa, 0x00, 0);
    let feat_lo = read_u32(bar_pa, 0x04);
    write_u32(bar_pa, 0x00, 1);
    let feat_hi = read_u32(bar_pa, 0x04);
    let dev_features = ((feat_hi as u64) << 32) | (feat_lo as u64);
    println!("virtio-gpu: device_features={:#018x}", dev_features);

    // ACK + DRIVER
    write_u8(bar_pa, 0x14, 1);
    write_u8(bar_pa, 0x14, 1 | 2);

    // Хотим: VIRTIO_F_VERSION_1 (32). Без VIRGL (bit 0) — 2D mode.
    let want: u64 = 1u64 << 32;
    if (want & !dev_features) != 0 {
        println!("virtio-gpu: ERROR: device doesn't support requested features");
        return None;
    }

    write_u32(bar_pa, 0x08, 0);
    write_u32(bar_pa, 0x0C, (want & 0xFFFFFFFF) as u32);
    write_u32(bar_pa, 0x08, 1);
    write_u32(bar_pa, 0x0C, (want >> 32) as u32);
    println!("virtio-gpu: wrote driver_features={:#018x}", want);

    write_u8(bar_pa, 0x14, 1 | 2 | 8);
    let s1 = read_u8(bar_pa, 0x14);
    println!("virtio-gpu: status after FEATURES_OK={:#x}", s1);
    if (s1 & 8) == 0 {
        println!("virtio-gpu: FEATURES_OK rejected");
        return None;
    }

    // === Setup control queue (queue 0) ===
    write_u16(bar_pa, 0x16, 0);
    let q0_size = read_u16(bar_pa, 0x18);
    println!("virtio-gpu: control queue size={}", q0_size);
    if q0_size == 0 { return None; }

    let desc_pa = phys::alloc_page()?;
    let avail_pa = phys::alloc_page()?;
    let used_pa = phys::alloc_page()?;
    core::ptr::write_bytes(desc_pa as *mut u8, 0, 4096);
    core::ptr::write_bytes(avail_pa as *mut u8, 0, 4096);
    core::ptr::write_bytes(used_pa as *mut u8, 0, 4096);

    write_u16(bar_pa, 0x16, 0);
    write_u16(bar_pa, 0x18, q0_size);
    write_u32(bar_pa, 0x20, desc_pa as u32);
    write_u32(bar_pa, 0x24, (desc_pa >> 32) as u32);
    write_u32(bar_pa, 0x28, avail_pa as u32);
    write_u32(bar_pa, 0x2C, (avail_pa >> 32) as u32);
    write_u32(bar_pa, 0x30, used_pa as u32);
    write_u32(bar_pa, 0x34, (used_pa >> 32) as u32);
    write_u16(bar_pa, 0x1C, 1);

    println!("virtio-gpu: control queue enabled: desc={:#x} avail={:#x} used={:#x}",
             desc_pa, avail_pa, used_pa);

    // === DRIVER_OK ===
    let s = read_u8(bar_pa, 0x14);
    write_u8(bar_pa, 0x14, s | 4);
    let s_final = read_u8(bar_pa, 0x14);
    println!("virtio-gpu: status={:#x} (expect 0xf)", s_final);
    if (s_final & 0x80) != 0 {
        println!("virtio-gpu: DEVICE FAILED");
        return None;
    }

    // DEVICE cap — read resolution if available
    // (не критично для init)

    Some(VirtioGpu {
        bar: bar_pa,
        bus, dev, func,
        dev_features,
        control_q: Virtqueue {
            desc_pa: desc_pa as u64,
            avail_pa: avail_pa as u64,
            used_pa: used_pa as u64,
            size: q0_size,
            next_avail: 0,
            last_used_idx: 0,
        },
        notify_off,
        notify_mult,
    })
}

const CMD_RESOURCE_CREATE_2D:         u32 = 0x0101;
const CMD_SET_SCANOUT:                u32 = 0x0103;
const CMD_TRANSFER_TO_HOST_2D:        u32 = 0x0105;
const CMD_RESOURCE_ATTACH_BACKING:    u32 = 0x0106;
const RESP_OK_NODATA:                 u32 = 0x1100;
const FORMAT_B8G8R8X8_UNORM:          u32 = 2;

unsafe fn notify_q(gpu: &VirtioGpu, qidx: u16) {
    write_u16(gpu.bar, 0x16, qidx);
    let noff = read_u16(gpu.bar, 0x1E);
    let addr = gpu.bar + gpu.notify_off as u64 + (noff as u64) * gpu.notify_mult as u64;
    core::arch::asm!("dsb sy");
    write_u16(addr, 0, qidx);
}

unsafe fn send_cmd(gpu: &mut VirtioGpu, cmd: &[u8], resp: &mut [u8]) -> bool {
    // Извлечь нужные поля ДО mutable borrow control_q
    let bar = gpu.bar;
    let notify_off = gpu.notify_off;
    let notify_mult = gpu.notify_mult;

    let q = &mut gpu.control_q;
    let size = q.size as usize;

    // Descriptor 0: cmd (read)
    let desc0 = &mut *((q.desc_pa + 0) as *mut VirtqDesc);
    desc0.addr = cmd.as_ptr() as u64;
    desc0.len = cmd.len() as u32;
    desc0.flags = 1;
    desc0.next = 1;

    // Descriptor 1: resp (write)
    let desc1 = &mut *((q.desc_pa + 16) as *mut VirtqDesc);
    desc1.addr = resp.as_mut_ptr() as u64;
    desc1.len = resp.len() as u32;
    desc1.flags = 2;
    desc1.next = 0;

    // Avail ring
    let avail = &mut *(q.avail_pa as *mut AvailPage);
    avail.ring[(q.next_avail as usize) % size] = 0;
    q.next_avail = q.next_avail.wrapping_add(1);
    avail.idx = q.next_avail;
    core::arch::asm!("dsb sy");

    // Notify — inline, без &gpu
    write_u16(bar, 0x16, 0);
    let noff = read_u16(bar, 0x1E);
    let addr = bar + notify_off as u64 + (noff as u64) * notify_mult as u64;
    core::arch::asm!("dsb sy");
    write_u16(addr, 0, 0);

    // Poll used ring
    let used = &mut *(q.used_pa as *mut UsedPage);
    let mut timeout = 0u64;
    while q.last_used_idx == used.idx {
        timeout += 1;
        if timeout > 10_000_000 { return false; }
        core::arch::asm!("yield", options(nostack, nomem));
    }
    q.last_used_idx = used.idx;

    let rt = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
    rt == RESP_OK_NODATA
}

pub unsafe fn setup_framebuffer(gpu: &mut VirtioGpu, fb_addr: u64, w: u32, h: u32) -> bool {
    let rid: u32 = 1;
    let mut resp = [0u8; 32];

    // CREATE_2D
    let mut c = [0u8; 40];
    c[0..4].copy_from_slice(&CMD_RESOURCE_CREATE_2D.to_le_bytes());
    c[24..28].copy_from_slice(&rid.to_le_bytes());
    c[28..32].copy_from_slice(&FORMAT_B8G8R8X8_UNORM.to_le_bytes());
    c[32..36].copy_from_slice(&w.to_le_bytes());
    c[36..40].copy_from_slice(&h.to_le_bytes());
    if !send_cmd(gpu, &c, &mut resp) { println!("CREATE_2D fail"); return false; }
    println!("virtio-gpu: RESOURCE_CREATE_2D OK");

    // ATTACH_BACKING
    let mut c = [0u8; 48];
    c[0..4].copy_from_slice(&CMD_RESOURCE_ATTACH_BACKING.to_le_bytes());
    c[24..28].copy_from_slice(&rid.to_le_bytes());
    c[28..32].copy_from_slice(&1u32.to_le_bytes());
    c[32..40].copy_from_slice(&fb_addr.to_le_bytes());
    c[40..44].copy_from_slice(&(w * h * 4).to_le_bytes());
    if !send_cmd(gpu, &c, &mut resp) { println!("ATTACH_BACKING fail"); return false; }
    println!("virtio-gpu: ATTACH_BACKING OK");

    // SET_SCANOUT (48 байт)
    // struct: hdr(24) + rect{ x, y, w, h }(16) + scanout_id(4) + resource_id(4)
    let mut c = [0u8; 48];
    c[0..4].copy_from_slice(&CMD_SET_SCANOUT.to_le_bytes());
    // rect: x, y, width, height
    c[24..28].copy_from_slice(&0u32.to_le_bytes());   // x
    c[28..32].copy_from_slice(&0u32.to_le_bytes());   // y
    c[32..36].copy_from_slice(&w.to_le_bytes());      // width
    c[36..40].copy_from_slice(&h.to_le_bytes());      // height
    // scanout_id, resource_id
    c[40..44].copy_from_slice(&0u32.to_le_bytes());   // scanout_id (0)
    c[44..48].copy_from_slice(&rid.to_le_bytes());    // resource_id (1)
    if !send_cmd(gpu, &c, &mut resp) { println!("SET_SCANOUT fail"); return false; }
    println!("virtio-gpu: SET_SCANOUT OK");
    true
}

pub unsafe fn flush_region(gpu: &mut VirtioGpu, x: u32, y: u32, w: u32, h: u32) -> bool {
    let mut resp = [0u8; 32];
    let stride: u64 = 640 * 4;
    let offset = (y as u64) * stride + (x as u64) * 4;

    // TRANSFER_TO_HOST_2D
    let mut c = [0u8; 56];
    c[0..4].copy_from_slice(&CMD_TRANSFER_TO_HOST_2D.to_le_bytes());
    c[24..28].copy_from_slice(&x.to_le_bytes());
    c[28..32].copy_from_slice(&y.to_le_bytes());
    c[32..36].copy_from_slice(&w.to_le_bytes());
    c[36..40].copy_from_slice(&h.to_le_bytes());
    c[40..48].copy_from_slice(&offset.to_le_bytes());   // <-- правильный offset
    c[48..52].copy_from_slice(&1u32.to_le_bytes());
    if !send_cmd(gpu, &c, &mut resp) { return false; }

    // RESOURCE_FLUSH (только region)
    let mut f = [0u8; 48];
    f[0..4].copy_from_slice(&0x0104u32.to_le_bytes());
    f[24..28].copy_from_slice(&x.to_le_bytes());
    f[28..32].copy_from_slice(&y.to_le_bytes());
    f[32..36].copy_from_slice(&w.to_le_bytes());
    f[36..40].copy_from_slice(&h.to_le_bytes());
    f[40..44].copy_from_slice(&1u32.to_le_bytes());
    send_cmd(gpu, &f, &mut resp)
}

pub unsafe fn flush(gpu: &mut VirtioGpu, w: u32, h: u32) -> bool {
    flush_region(gpu, 0, 0, w, h)
}

static mut GPU: Option<VirtioGpu> = None;

pub unsafe fn set_global(g: VirtioGpu) {
    GPU = Some(g);
}

pub fn get_global_mut() -> Option<&'static mut VirtioGpu> {
    unsafe { GPU.as_mut() }
}