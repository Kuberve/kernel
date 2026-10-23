use crate::println;
use crate::fs::vfs::{VfsError, VfsResult};
use crate::drivers::nvme::{Nvme, read_blocks, write_blocks};
use crate::arch::aarch64::phys;

#[repr(C, align(4096))]
pub struct NvmeBuf(pub [u8; 4096]);

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

    /// Прочитать один кластер в буфер `buf_pa`.
    pub unsafe fn read_cluster(&self, cluster: u32, buf_pa: u64) -> bool {
        let nvme = &*self.bar_nvme;
        let start_sector = self.first_data_sector as u64
            + (cluster as u64 - 2) * self.sectors_per_cluster as u64;
        let num_sectors = self.sectors_per_cluster as u16;
        read_blocks(nvme, 1, start_sector, num_sectors, buf_pa)
    }

    /// Записать один кластер из буфера `buf_pa` (512 байт).
    pub unsafe fn write_cluster(&self, cluster: u32, buf_pa: u64) -> bool {
        let nvme = &*self.bar_nvme;
        let start_sector = self.first_data_sector as u64
            + (cluster as u64 - 2) * self.sectors_per_cluster as u64;
        let num_sectors = self.sectors_per_cluster as u16;
        crate::drivers::nvme::write_blocks(nvme, 1, start_sector, num_sectors, buf_pa)
    }

    /// Прочитать FAT entry → следующий кластер. 0x0FFFFFFF = EOC.
    pub unsafe fn next_cluster(&self, cluster: u32) -> u32 {
        let nvme = &*self.bar_nvme;
        let fat_sector = self.reserved_sectors as u64
            + ((cluster as u64) * 4) / self.bytes_per_sector as u64;

        static mut SCRATCH: NvmeBuf = NvmeBuf([0; 4096]);
        let buf_pa = &raw mut SCRATCH as *mut NvmeBuf as u64;

        if !read_blocks(nvme, 1, fat_sector, 1, buf_pa) {
            return 0x0FFFFFFF;
        }
        let offset_in_sector = ((cluster as usize) * 4) % self.bytes_per_sector as usize;
        let buf = core::slice::from_raw_parts(buf_pa as *const u8, self.bytes_per_sector as usize);
        read_u32(buf, offset_in_sector) & 0x0FFFFFFF
    }

    /// Записать FAT entry в обе копии FAT. Сохраняет reserved верхние 4 бита.
    pub unsafe fn write_fat_entry(&self, cluster: u32, value: u32) -> bool {
        let nvme = &*self.bar_nvme;
        let entry_offset = (cluster as u64) * 4;
        let sector_in_fat = entry_offset / self.bytes_per_sector as u64;
        let offset_in_sector = (entry_offset % self.bytes_per_sector as u64) as usize;

        static mut FAT_BUF: NvmeBuf = NvmeBuf([0; 4096]);
        let buf_pa = &raw mut FAT_BUF as *mut NvmeBuf as u64;

        // FAT1
        let fat1_sector = self.reserved_sectors as u64 + sector_in_fat;
        if !read_blocks(nvme, 1, fat1_sector, 1, buf_pa) { return false; }

        let buf = core::slice::from_raw_parts_mut(buf_pa as *mut u8, 512);
        let old = read_u32(buf, offset_in_sector);
        let new = (old & 0xF0000000) | (value & 0x0FFFFFFF);
        let bytes = new.to_le_bytes();
        for i in 0..4 {
            buf[offset_in_sector + i] = bytes[i];
        }
        if !write_blocks(nvme, 1, fat1_sector, 1, buf_pa) { return false; }

        // FAT2 (same offset, but + sectors_per_fat from FAT1)
        let fat2_sector = fat1_sector + self.sectors_per_fat as u64;
        if !write_blocks(nvme, 1, fat2_sector, 1, buf_pa) { return false; }

        true
    }

    /// Найти свободный кластер (FAT entry = 0), пометить EOC, вернуть номер.
    pub unsafe fn alloc_cluster(&self) -> Option<u32> {
        let max_cluster = self.total_sectors / (self.sectors_per_cluster as u32) - 2;
        for cluster in 2..max_cluster {
            let next = self.next_cluster(cluster);
            if next == 0 {
                if self.write_fat_entry(cluster, 0x0FFFFFFF) {
                    return Some(cluster);
                }
                return None;
            }
        }
        None
    }

    /// Найти свободное место в директории (entry с первым байтом 0x00 или 0xE5).
    /// Возвращает (cluster, offset_in_cluster, entry_index).
    pub unsafe fn find_free_dir_entry(&self, dir_cluster: u32) -> Option<(u32, u32, usize)> {
        let mut cluster = dir_cluster;
        while cluster >= 2 && cluster < 0x0FFFFFF8 {
            static mut DIR_SCRATCH: NvmeBuf = NvmeBuf([0; 4096]);
            let buf_pa = &raw mut DIR_SCRATCH as *mut NvmeBuf as u64;
            if !self.read_cluster(cluster, buf_pa) { return None; }
            let cluster_size = self.bytes_per_sector as usize * self.sectors_per_cluster as usize;
            let entries = cluster_size / 32;
            let buf = core::slice::from_raw_parts(buf_pa as *const u8, cluster_size);
            for i in 0..entries {
                let first = buf[i * 32];
                if first == 0x00 || first == 0xE5 {
                    return Some((cluster, i as u32 * 32, i));
                }
            }
            cluster = self.next_cluster(cluster);
        }
        None
    }

    /// Создать пустой файл. Возвращает (first_cluster, size=0).
    /// Если файл уже существует — возвращает его entry.
    pub unsafe fn create_file(&self, dir_cluster: u32, path: &str) -> Option<(u32, u32)> {
        // Проверить существование
        if let Some((c, s, is_dir)) = self.find_entry(dir_cluster, path) {
            if is_dir { return None; }
            return Some((c, s));
        }

        let (dir_cl, _entry_offset, entry_idx) = self.find_free_dir_entry(dir_cluster)?;
        let first_cluster = self.alloc_cluster()?;

        // Сформировать 8.3
        let mut target = [b' '; 11];
        let bytes = path.as_bytes();
        let mut i = 0; let mut j = 0;
        while i < bytes.len() && j < 8 {
            let c = bytes[i];
            if c == b'.' { break; }
            target[j] = if c >= b'a' && c <= b'z' { c - 32 } else { c };
            i += 1; j += 1;
        }
        while i < bytes.len() && bytes[i] != b'.' { i += 1; }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            let mut k = 8;
            while i < bytes.len() && k < 11 {
                let c = bytes[i];
                target[k] = if c >= b'a' && c <= b'z' { c - 32 } else { c };
                i += 1; k += 1;
            }
        }

        // Прочитать dir cluster, обновить entry, записать обратно
        static mut CRE_BUF: NvmeBuf = NvmeBuf([0; 4096]);
        let buf_pa = &raw mut CRE_BUF as *mut NvmeBuf as u64;
        if !self.read_cluster(dir_cl, buf_pa) { return None; }
        let buf = core::slice::from_raw_parts_mut(buf_pa as *mut u8, 512);

        let off = entry_idx * 32;
        for k in 0..11 { buf[off + k] = target[k]; }
        buf[off + 11] = 0x20;  // attr: archive (обычный файл)
        buf[off + 12] = 0;
        buf[off + 13] = 0;
        for k in 0..4 { buf[off + 14 + k] = 0; }  // CrtTime/Date
        for k in 0..2 { buf[off + 18 + k] = 0; }  // LstAccDate
        // FstClusHI
        let hi = ((first_cluster >> 16) & 0xFFFF) as u16;
        buf[off + 20] = (hi & 0xFF) as u8;
        buf[off + 21] = ((hi >> 8) & 0xFF) as u8;
        for k in 0..4 { buf[off + 22 + k] = 0; }  // WrtTime/Date
        // FstClusLO
        let lo = (first_cluster & 0xFFFF) as u16;
        buf[off + 26] = (lo & 0xFF) as u8;
        buf[off + 27] = ((lo >> 8) & 0xFF) as u8;
        // FileSize = 0
        for k in 0..4 { buf[off + 28 + k] = 0; }

        if !self.write_cluster(dir_cl, buf_pa) { return None; }

        Some((first_cluster, 0))
    }

    /// Обновить size (4 байта, offset 28) в dir entry по first_cluster файла.
    pub unsafe fn update_dir_size(&self, dir_cluster: u32, file_first_cluster: u32, new_size: u32) -> bool {
        let mut cluster = dir_cluster;
        while cluster >= 2 && cluster < 0x0FFFFFF8 {
            static mut UPD_BUF: NvmeBuf = NvmeBuf([0; 4096]);
            let buf_pa = &raw mut UPD_BUF as *mut NvmeBuf as u64;
            if !self.read_cluster(cluster, buf_pa) { return false; }

            let cluster_size = self.bytes_per_sector as usize * self.sectors_per_cluster as usize;
            let entries = cluster_size / 32;
            let buf = core::slice::from_raw_parts_mut(buf_pa as *mut u8, cluster_size);

            for i in 0..entries {
                let off = i * 32;
                let first = buf[off];
                if first == 0x00 { return false; }
                if first == 0xE5 { continue; }
                let attr = buf[off + 11];
                if attr == 0x0F { continue; }
                if attr & 0x08 != 0 { continue; }

                let hi = read_u16(buf, off + 20) as u32;
                let lo = read_u16(buf, off + 26) as u32;
                let entry_cluster = (hi << 16) | lo;

                if entry_cluster == file_first_cluster {
                    let bytes = new_size.to_le_bytes();
                    buf[off + 28] = bytes[0];
                    buf[off + 29] = bytes[1];
                    buf[off + 30] = bytes[2];
                    buf[off + 31] = bytes[3];
                    return self.write_cluster(cluster, buf_pa);
                }
            }
            cluster = self.next_cluster(cluster);
        }
        false
    }

    /// Прочитать содержимое директории (начиная с start_cluster), вывести entries.
    pub unsafe fn readdir(&self, start_cluster: u32) {
        let mut cluster = start_cluster;
        let mut total = 0;
        let max_entries = 500;

        while cluster >= 2 && cluster < 0x0FFFFFF8 && total < max_entries {
            static mut DIR_BUF: NvmeBuf = NvmeBuf([0; 4096]);
            let buf_pa = &raw mut DIR_BUF as *mut NvmeBuf as u64;
            if !self.read_cluster(cluster, buf_pa as u64) {
                println!("FAT32: read cluster {} failed", cluster);
                return;
            }

            let cluster_size = self.bytes_per_sector as usize * self.sectors_per_cluster as usize;
            let entries_per_cluster = cluster_size / 32;
            let buf = core::slice::from_raw_parts(buf_pa as *const u8, cluster_size);

            for i in 0..entries_per_cluster {
                let e = &buf[i * 32..(i + 1) * 32];
                let first = e[0];
                if first == 0x00 {
                    return;  // конец директории
                }
                if first == 0xE5 {
                    continue;  // удалён
                }
                let attr = e[11];
                if attr == 0x0F {
                    continue;  // LFN, игнорируем
                }
                if attr & 0x08 != 0 {
                    continue;  // volume label
                }

                // Имя 8.3
                let mut name = [0u8; 8];
                let mut n = 0;
                for j in 0..8 {
                    if e[j] == b' ' { break; }
                    name[n] = e[j]; n += 1;
                }
                let mut ext = [0u8; 3];
                let mut m = 0;
                for j in 8..11 {
                    if e[j] == b' ' { break; }
                    ext[m] = e[j]; m += 1;
                }

                let name_str = core::str::from_utf8(&name[..n]).unwrap_or("?");
                let ext_str = core::str::from_utf8(&ext[..m]).unwrap_or("");

                let cluster_hi = read_u16(e, 20) as u32;
                let cluster_lo = read_u16(e, 26) as u32;
                let file_cluster = (cluster_hi << 16) | cluster_lo;
                let size = read_u32(e, 28);
                let is_dir = attr & 0x10 != 0;

                if is_dir {
                    println!("  [DIR]  {:<12} cluster={}", name_str, file_cluster);
                } else if m > 0 {
                    println!("  [FILE] {}.{:<3} size={:<6} cluster={}",
                        name_str, ext_str, size, file_cluster);
                } else {
                    println!("  [FILE] {:<12} size={:<6} cluster={}",
                        name_str, size, file_cluster);
                }
                total += 1;
                if total >= max_entries { return; }
            }

            cluster = self.next_cluster(cluster);
        }
    }

    /// Найти entry по имени (8.3, case-insensitive) в директории `dir_cluster`.
    /// Возвращает (first_cluster, size, is_dir).
    pub unsafe fn find_entry(
        &self,
        dir_cluster: u32,
        name: &str,
    ) -> Option<(u32, u32, bool)> {
        // Нормализовать в 8.3: "hello.txt" → "HELLO   TXT"
        let mut target = [b' '; 11];
        let bytes = name.as_bytes();
        let mut i = 0;
        let mut j = 0;
        while i < bytes.len() && j < 8 {
            let c = bytes[i];
            if c == b'.' { break; }
            target[j] = if c >= b'a' && c <= b'z' { c - 32 } else { c };
            i += 1; j += 1;
        }
        // ext
        while i < bytes.len() && bytes[i] != b'.' { i += 1; }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            let mut k = 8;
            while i < bytes.len() && k < 11 {
                let c = bytes[i];
                target[k] = if c >= b'a' && c <= b'z' { c - 32 } else { c };
                i += 1; k += 1;
            }
        }

        let mut cluster = dir_cluster;
        while cluster >= 2 && cluster < 0x0FFFFFF8 {
            let buf_pa = phys::alloc_page()?;
            if !self.read_cluster(cluster, buf_pa as u64) {
                return None;
            }
            let cluster_size = self.bytes_per_sector as usize * self.sectors_per_cluster as usize;
            let entries = cluster_size / 32;
            let buf = core::slice::from_raw_parts(buf_pa as *const u8, cluster_size);

            for e_idx in 0..entries {
                let e = &buf[e_idx * 32..(e_idx + 1) * 32];
                let first = e[0];
                if first == 0x00 { return None; }
                if first == 0xE5 { continue; }
                let attr = e[11];
                if attr == 0x0F { continue; }
                if attr & 0x08 != 0 { continue; }

                // Сравнить 11 байт name+ext
                let mut matched = true;
                for k in 0..11 {
                    if e[k] != target[k] { matched = false; break; }
                }
                if matched {
                    let hi = read_u16(e, 20) as u32;
                    let lo = read_u16(e, 26) as u32;
                    let file_cluster = (hi << 16) | lo;
                    let size = read_u32(e, 28);
                    let is_dir = attr & 0x10 != 0;
                    return Some((file_cluster, size, is_dir));
                }
            }
            cluster = self.next_cluster(cluster);
        }
        None
    }

    /// Прочитать файл целиком в буфер `out_pa` (должен быть >= size байт).
    /// Возвращает реально прочитанное число байт.
    pub unsafe fn read_file(
        &self,
        first_cluster: u32,
        size: u32,
        out_pa: u64,
    ) -> Option<usize> {
        if size == 0 { return Some(0); }
        let cluster_size = self.bytes_per_sector as usize * self.sectors_per_cluster as usize;
        let mut cluster = first_cluster;
        let mut written = 0usize;

        static mut TMP: NvmeBuf = NvmeBuf([0; 4096]);
        let tmp_pa = &raw mut TMP as *mut NvmeBuf as u64;
        let out = out_pa as *mut u8;

        while cluster >= 2 && cluster < 0x0FFFFFF8 && written < size as usize {
            if !self.read_cluster(cluster, tmp_pa) {
                return None;
            }
            let remain = size as usize - written;
            let copy_n = if remain < cluster_size { remain } else { cluster_size };
            let src = tmp_pa as *const u8;
            for i in 0..copy_n {
                core::ptr::write_volatile(out.add(written + i), core::ptr::read_volatile(src.add(i)));
            }
            written += copy_n;
            cluster = self.next_cluster(cluster);
        }
        if written == size as usize {
            Some(written)
        } else {
            None
        }
    }   
}

// ============================================================
// Глобальный mounted FS
// ============================================================

pub static mut FS: Option<Fat32> = None;

pub unsafe fn mount_global(nvme: &Nvme) -> VfsResult<()> {
    let fs = mount(nvme)?;
    FS = Some(fs);
    Ok(())
}

pub fn get() -> Option<&'static Fat32> {
    unsafe { FS.as_ref() }
}