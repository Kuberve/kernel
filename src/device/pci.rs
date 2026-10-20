use crate::println;

/// QEMU virt (ARM64): ECAM base.
/// Для x86_64 будет другой адрес — вынести в arch::pci_ecam_base() когда будет x86.
const ECAM_BASE: u64 = 0x4010000000;
const ECAM_SIZE: u64 = 256 * 1024 * 1024;

/// PCIe config space access via ECAM.
/// Offset = (bus << 20) | (dev << 15) | (func << 12) | reg
fn ecam_addr(bus: u8, dev: u8, func: u8, reg: u16) -> u64 {
    ECAM_BASE
        + ((bus as u64) << 20)
        + ((dev as u64) << 15)
        + ((func as u64) << 12)
        + (reg as u64)
}

fn config_read32(bus: u8, dev: u8, func: u8, reg: u16) -> u32 {
    unsafe {
        let ptr = ecam_addr(bus, dev, func, reg) as *const u32;
        core::ptr::read_volatile(ptr)
    }
}

fn config_write32(bus: u8, dev: u8, func: u8, reg: u16, val: u32) {
    unsafe {
        let ptr = ecam_addr(bus, dev, func, reg) as *mut u32;
        core::ptr::write_volatile(ptr, val);
    }
}

/// Инициализация: найти NVMe контроллер и вернуть его BAR0.
pub unsafe fn init() -> Option<u64> {
    println!("PCIe: scanning ECAM at {:#x}...", ECAM_BASE);

    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            for func in 0..8u8 {
                let vendor = config_read32(bus, dev, func, 0x00) & 0xFFFF;
                if vendor == 0xFFFF {
                    continue;
                }

                let class = config_read32(bus, dev, func, 0x08);
                let base_class = (class >> 24) & 0xFF;
                let sub_class = (class >> 16) & 0xFF;
                let prog_if = (class >> 8) & 0xFF;

                // NVMe: base=0x01, sub=0x08, prog_if=0x02
                if base_class == 0x01 && sub_class == 0x08 && prog_if == 0x02 {
                    println!(
                        "PCIe: NVMe found at bus={} dev={} func={} vendor={:#x}",
                        bus, dev, func, vendor
                    );

                    let bar0 = config_read32(bus, dev, func, 0x10);
                    let bar1 = config_read32(bus, dev, func, 0x14);

                    let is_mmio  = (bar0 & 1) == 0;
                    let is_64bit = ((bar0 >> 1) & 3) == 2;
                    let addr_lo  = (bar0 as u64) & !0xF;
                    let addr_hi  = bar1 as u64;
                    let is_unassigned = is_mmio && addr_lo == 0 && addr_hi == 0;

                    println!(
                        "PCIe: bar0={:#x} bar1={:#x} mmio={} 64bit={} unassigned={}",
                        bar0, bar1, is_mmio, is_64bit, is_unassigned
                    );

                    let bar_pa = if is_unassigned {
                        // Назначаем на фиксированный адрес (одно устройство — просто)
                        let target: u64 = 0x10000000;
                        let new_bar0 = (target as u32)
                            | if is_64bit { 0x4 } else { 0 };
                        config_write32(bus, dev, func, 0x10, new_bar0);
                        if is_64bit {
                            config_write32(bus, dev, func, 0x14, (target >> 32) as u32);
                        }
                        let rb0 = config_read32(bus, dev, func, 0x10);
                        let rb1 = config_read32(bus, dev, func, 0x14);
                        println!("PCIe: BAR0 readback = {:#x}{:08x}", rb1, rb0);
                        target
                    } else if is_64bit {
                        (addr_hi << 32) | addr_lo
                    } else if is_mmio {
                        addr_lo
                    } else {
                        println!("PCIe: BAR0 is I/O space (unsupported)");
                        continue;
                    };

                    println!("PCIe: BAR0 (final) = {:#x}", bar_pa);

                    let mut cmd = config_read32(bus, dev, func, 0x04);
                    cmd |= (1 << 1) | (1 << 2);
                    config_write32(bus, dev, func, 0x04, cmd);
                    println!("PCIe: Command = {:#x}", cmd);

                    return Some(bar_pa);
                }
            }
        }
    }

    println!("PCIe: NVMe not found");
    None
}

/// Прочитать размер BAR через запись всех единиц.
/// Возвращает маску размера (последний установленный бит).
fn bar_size_mask(bus: u8, dev: u8, func: u8, bar_reg: u16) -> u32 {
    let orig = config_read32(bus, dev, func, bar_reg);
    config_write32(bus, dev, func, bar_reg, 0xFFFFFFFF);
    let mask = config_read32(bus, dev, func, bar_reg);
    config_write32(bus, dev, func, bar_reg, orig);
    mask & !0xF   // младшие 4 бита — флаги, не часть маски
}

/// Найти свободный регион MMIO для 64-bit BAR.
/// Сканируем 0x10000000..0x40000000 с шагом size, проверяем что не пересекается.
unsafe fn assign_bar64(bus: u8, dev: u8, func: u8, bar_reg: u16, size: u64) -> Option<u64> {
    if size == 0 || (size & (size - 1)) != 0 {
        return None;  // должно быть степенью двойки
    }
    // Простой bump allocator: начинаем с 0x10000000
    // (в реальном kernel — proper resource allocator)
    let mut addr: u64 = 0x10000000;
    // Выровнять
    addr = (addr + size - 1) & !(size - 1);

    // Проверка: не пересекается с другими BAR (упрощённо — просто даём адрес)
    let bar0 = (addr as u32) | 0x4;   // bit 2 = 64-bit MMIO
    let bar1 = (addr >> 32) as u32;

    config_write32(bus, dev, func, bar_reg, bar0);
    config_write32(bus, dev, func, bar_reg + 4, bar1);

    Some(addr)
}