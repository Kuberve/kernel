use crate::println;
use crate::device::vfs::{VfsError, VfsResult, MountInfo};
use crate::device::nvme::{Nvme, read_blocks};
use crate::arch::aarch64::phys;

pub struct Fat32 {
    pub bar_nvme: *const Nvme,   // для чтения блоков
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub num_fats: u8,
    pub sectors_per_fat: u32,
    pub root_cluster: u32,
    pub total_sectors: u32,
    pub label: [u8; 11],
    pub first_data_sector: u32,
}

fn read_u16(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}

fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

/// Прочитать один блок (512 байт) с диска через NVMe.
unsafe fn read_sector(nvme: &Nvme, lba: u64, buf_pa: u64) -> bool {
    read_blocks(nvme, 1, lba, 1, buf_pa)
}

/// Mount: прочитать boot sector, проверить сигнатуру, заполнить Fat32.
pub unsafe fn mount(nvme: &Nvme) -> VfsResult<Fat32> {
    // Выделить буфер под boot sector
    let bs_pa = phys::alloc_page().ok_or(VfsError::IoError)?;
    core::ptr::write_bytes(bs_pa as *mut u8, 0, 4096);

    if !read_sector(nvme, 0, bs_pa as u64) {
        return Err(VfsError::IoError);
    }

    let bs = core::slice::from_raw_parts(bs_pa as *const u8, 512);

    // Проверить сигнатуру 0x55 0xAA на offset 0x1FE
    if bs[0x1FE] != 0x55 || bs[0x1FF] != 0xAA {
        println!("FAT32: bad boot signature {:#x} {:#x}", bs[0x1FE], bs[0x1FF]);
        return Err(VfsError::InvalidFs);
    }

    // Проверить FS type на offset 0x52 (8 байт)
    let fs_type = &bs[0x52..0x5A];
    if fs_type != b"FAT32   " {
        println!("FAT32: unexpected fs type {:?}", fs_type);
        return Err(VfsError::InvalidFs);
    }

    let bytes_per_sector = read_u16(bs, 0x0B);
    let sectors_per_cluster = bs[0x0D];
    let reserved_sectors = read_u16(bs, 0x0E);
    let num_fats = bs[0x10];
    let total_sectors_32 = read_u32(bs, 0x20);
    let sectors_per_fat = read_u32(bs, 0x24);
    let root_cluster = read_u32(bs, 0x2C);
    let mut label = [0u8; 11];
    label.copy_from_slice(&bs[0x47..0x52]);

    let first_data_sector = reserved_sectors as u32
        + (num_fats as u32) * sectors_per_fat;

    let cluster_size = bytes_per_sector as u32 * sectors_per_cluster as u32;
    let data_sectors = total_sectors_32.saturating_sub(first_data_sector);
    let total_clusters = data_sectors / (sectors_per_cluster as u32);

    println!("FAT32: mount OK");
    println!("  bytes_per_sector  = {}", bytes_per_sector);
    println!("  sectors_per_cluster = {}", sectors_per_cluster);
    println!("  cluster_size      = {} bytes", cluster_size);
    println!("  reserved_sectors  = {}", reserved_sectors);
    println!("  num_fats          = {}", num_fats);
    println!("  sectors_per_fat   = {}", sectors_per_fat);
    println!("  root_cluster      = {}", root_cluster);
    println!("  total_sectors     = {}", total_sectors_32);
    println!("  first_data_sector = {}", first_data_sector);
    println!("  total_clusters    = {}", total_clusters);

    // Вернуть страницу в phys allocator? Нет — оставим, будем использовать как кэш
    // (позже заменим на proper free)

    Ok(Fat32 {
        bar_nvme: nvme as *const Nvme,
        bytes_per_sector,
        sectors_per_cluster,
        reserved_sectors,
        num_fats,
        sectors_per_fat,
        root_cluster,
        total_sectors: total_sectors_32,
        label,
        first_data_sector,
    })
}

impl Fat32 {
    /// Смещение (в байтах от начала диска) для кластера `cluster`.
    pub fn cluster_offset(&self, cluster: u32) -> u64 {
        let sector = self.first_data_sector as u64
            + (cluster as u64 - 2) * self.sectors_per_cluster as u64;
        sector * self.bytes_per_sector as u64
    }

    /// Label как строка (trim trailing spaces).
    pub fn label_str(&self) -> &str {
        let s = core::str::from_utf8(&self.label).unwrap_or("?");
        s.trim_end()
    }
}