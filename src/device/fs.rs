use crate::println;
use crate::device::fat32;

pub const MAX_FDS: usize = 32;

static mut FD_USED: [bool; MAX_FDS] = [false; MAX_FDS];
static mut FD_FIRST_CLUSTER: [u32; MAX_FDS] = [0; MAX_FDS];
static mut FD_SIZE: [u32; MAX_FDS] = [0; MAX_FDS];
static mut FD_OFFSET: [u32; MAX_FDS] = [0; MAX_FDS];

static mut READ_BUF: [u8; 4096] = [0; 4096];
static mut WRITE_BUF: [u8; 4096] = [0; 4096];

pub unsafe fn fd_open(path: &str) -> i64 {
    let fs = match fat32::get() {
        Some(f) => f,
        None => return -1,
    };
    let (first_cluster, size, is_dir) = match fs.find_entry(fs.root_cluster, path) {
        Some(e) => e,
        None => return -1,
    };
    if is_dir { return -1; }

    for i in 0..MAX_FDS {
        if !FD_USED[i] {
            FD_USED[i] = true;
            FD_FIRST_CLUSTER[i] = first_cluster;
            FD_SIZE[i] = size;
            FD_OFFSET[i] = 0;
            return i as i64;
        }
    }
    -1
}

pub unsafe fn fd_read(fd: u64, buf: u64, len: u64) -> i64 {
    if fd as usize >= MAX_FDS || !FD_USED[fd as usize] { return -1; }
    let fd = fd as usize;
    let offset = FD_OFFSET[fd];
    let size = FD_SIZE[fd];
    if offset >= size { return 0; }

    let to_read = core::cmp::min(len as u32, size - offset);
    let fs = match fat32::get() {
        Some(f) => f,
        None => return -1,
    };

    let cluster_size = (fs.bytes_per_sector as u32) * (fs.sectors_per_cluster as u32);
    if cluster_size as usize > READ_BUF.len() { return -1; }

    // Traverse до нужного кластера
    let skip = offset / cluster_size;
    let mut cluster = FD_FIRST_CLUSTER[fd];
    for _ in 0..skip {
        cluster = fs.next_cluster(cluster);
        if cluster >= 0x0FFFFFF8 { return -1; }
    }
    let mut in_cluster = (offset % cluster_size) as usize;

    let buf_pa = &raw mut READ_BUF as u64;
    let out = buf as *mut u8;
    let mut written = 0u32;
    let mut remaining = to_read;

    while remaining > 0 {
        if !fs.read_cluster(cluster, buf_pa) { return -1; }
        let avail = cluster_size as usize - in_cluster;
        let to_copy = core::cmp::min(avail, remaining as usize);
        let src = buf_pa as *const u8;
        for i in 0..to_copy {
            core::ptr::write_volatile(
                out.add(written as usize + i),
                core::ptr::read_volatile(src.add(in_cluster + i)),
            );
        }
        written += to_copy as u32;
        remaining -= to_copy as u32;
        cluster = fs.next_cluster(cluster);
        in_cluster = 0;
        if cluster >= 0x0FFFFFF8 { break; }
    }

    FD_OFFSET[fd] += written;
    written as i64
}

pub unsafe fn fd_close(fd: u64) -> i64 {
    if fd as usize >= MAX_FDS || !FD_USED[fd as usize] { return -1; }
    FD_USED[fd as usize] = false;
    0
}

pub unsafe fn fd_write(fd: u64, buf: u64, len: u64) -> i64 {
    if fd as usize >= MAX_FDS || !FD_USED[fd as usize] { return -1; }
    let fd = fd as usize;
    let offset = FD_OFFSET[fd];
    let size = FD_SIZE[fd];

    // MVP: только перезапись в пределах существующего размера
    if offset >= size {
        return -1;  // за пределами файла
    }
    let max_write = size - offset;
    let to_write = core::cmp::min(len as u32, max_write);

    let fs = match fat32::get() {
        Some(f) => f,
        None => return -1,
    };

    let cluster_size = (fs.bytes_per_sector as u32) * (fs.sectors_per_cluster as u32);
    if cluster_size as usize > WRITE_BUF.len() { return -1; }

    // Найти кластер, в котором начинается offset
    let skip = offset / cluster_size;
    let mut cluster = FD_FIRST_CLUSTER[fd];
    for _ in 0..skip {
        cluster = fs.next_cluster(cluster);
        if cluster >= 0x0FFFFFF8 { return -1; }
    }
    let mut in_cluster = (offset % cluster_size) as usize;

    let buf_pa = &raw mut WRITE_BUF as u64;
    let src = buf as *const u8;
    let mut written = 0u32;
    let mut remaining = to_write;

    while remaining > 0 {
        // Прочитать текущий кластер (нужен для partial write)
        if !fs.read_cluster(cluster, buf_pa) { return -1; }

        let avail = cluster_size as usize - in_cluster;
        let to_copy = core::cmp::min(avail, remaining as usize);

        // Копировать данные в буфер
        let dst = buf_pa as *mut u8;
        for i in 0..to_copy {
            core::ptr::write_volatile(
                dst.add(in_cluster + i),
                core::ptr::read_volatile(src.add(written as usize + i)),
            );
        }

        // Записать кластер обратно
        if !fs.write_cluster(cluster, buf_pa) { return -1; }

        written += to_copy as u32;
        remaining -= to_copy as u32;
        cluster = fs.next_cluster(cluster);
        in_cluster = 0;
        if cluster >= 0x0FFFFFF8 { break; }
    }

    FD_OFFSET[fd] += written;
    written as i64
}