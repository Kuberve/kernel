use crate::device::fat32;
use crate::device::fat32::NvmeBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FdKind {
    File,
    Socket,
    TcpSocket,
}

pub const MAX_FDS: usize = 32;

static mut FD_USED: [bool; MAX_FDS] = [false; MAX_FDS];
static mut FD_KIND: [FdKind; MAX_FDS] = [FdKind::File; MAX_FDS];
static mut FD_SOCKET_ID: [usize; MAX_FDS] = [0; MAX_FDS];
static mut FD_FIRST_CLUSTER: [u32; MAX_FDS] = [0; MAX_FDS];
static mut FD_SIZE: [u32; MAX_FDS] = [0; MAX_FDS];
static mut FD_OFFSET: [u32; MAX_FDS] = [0; MAX_FDS];

static mut READ_BUF:  NvmeBuf = NvmeBuf([0; 4096]);
static mut WRITE_BUF: NvmeBuf = NvmeBuf([0; 4096]);
static mut DIR_BUF:   NvmeBuf = NvmeBuf([0; 4096]);

pub unsafe fn fd_open(path: &str, flags: u64) -> i64 {
    let fs = match fat32::get() {
        Some(f) => f,
        None => return -1,
    };

    let (first_cluster, size) = match fs.find_entry(fs.root_cluster, path) {
        Some((c, s, is_dir)) => {
            if is_dir { return -1; }
            (c, s)
        }
        None => {
            if flags & kuberve_abi::flags::O_CREAT != 0 {
                match fs.create_file(fs.root_cluster, path) {
                    Some((c, s)) => (c, s),
                    None => return -1,
                }
            } else {
                return -1;
            }
        }
    };

    for i in 0..MAX_FDS {
        if !FD_USED[i] {
            FD_USED[i] = true;
            FD_KIND[i] = FdKind::File;
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
    if cluster_size as usize > READ_BUF.0.len() { return -1; }

    // Traverse до нужного кластера
    let skip = offset / cluster_size;
    let mut cluster = FD_FIRST_CLUSTER[fd];
    for _ in 0..skip {
        cluster = fs.next_cluster(cluster);
        if cluster >= 0x0FFFFFF8 { return -1; }
    }
    let mut in_cluster = (offset % cluster_size) as usize;

    let buf_pa = &raw mut READ_BUF as *mut NvmeBuf as u64;
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
    let kind = FD_KIND[fd as usize];
    if kind == FdKind::Socket {
        let sid = FD_SOCKET_ID[fd as usize];
        crate::network::socket::close(sid);
    }
    if kind == FdKind::TcpSocket {
        crate::network::tcp::close(fd as i64);
    }
    FD_USED[fd as usize] = false;
    0
}

pub unsafe fn fd_write(fd: u64, buf: u64, len: u64) -> i64 {
    if fd as usize >= MAX_FDS || !FD_USED[fd as usize] { return -1; }
    let fd = fd as usize;
    let offset = FD_OFFSET[fd];
    let size = FD_SIZE[fd];
    let first_cluster = FD_FIRST_CLUSTER[fd];

    let fs = match fat32::get() {
        Some(f) => f,
        None => return -1,
    };

    let cluster_size = (fs.bytes_per_sector as u32) * (fs.sectors_per_cluster as u32);
    if cluster_size as usize > 4096 { return -1; }

    let new_size = core::cmp::max(size, offset + len as u32);
    let n_needed = ((new_size + cluster_size - 1) / cluster_size) as u32;
    if n_needed > 32 { return -1; }

    // Собрать существующий chain
    let mut clusters = [0u32; 32];
    let mut n = 0u32;
    let mut cur = first_cluster;
    while n < n_needed && cur >= 2 && cur < 0x0FFFFFF8 {
        clusters[n as usize] = cur;
        n += 1;
        if n >= n_needed { break; }
        cur = fs.next_cluster(cur);
    }
    if n == 0 { return -1; }

    // Аллоцировать новые, если extend
    while n < n_needed {
        let new_c = match fs.alloc_cluster() {
            Some(c) => c,
            None => return -1,
        };
        if !fs.write_fat_entry(clusters[(n - 1) as usize], new_c) { return -1; }
        clusters[n as usize] = new_c;
        n += 1;
    }

    // Записать данные по кластерам
    let src = buf as *const u8;

    for i in 0..n {
        let cluster_start = i * cluster_size;
        let cluster_end = cluster_start + cluster_size;
        let write_start = core::cmp::max(offset, cluster_start);
        let write_end = core::cmp::min(offset + len as u32, cluster_end);
        if write_start >= write_end { continue; }

        // Прочитать кластер (для partial write)
        if !fs.read_cluster(clusters[i as usize], &raw mut WRITE_BUF as *mut NvmeBuf as u64) { return -1; }

        let buf_slice = core::slice::from_raw_parts_mut(&raw mut WRITE_BUF as *mut u8, cluster_size as usize);
        for p in write_start..write_end {
            buf_slice[(p - cluster_start) as usize] =
                core::ptr::read_volatile(src.add((p - offset) as usize));
        }

        if !fs.write_cluster(clusters[i as usize], &raw mut WRITE_BUF as *mut NvmeBuf as u64) { return -1; }
    }

    // Обновить size в dir entry
    if new_size != size {
        if !fs.update_dir_size(fs.root_cluster, first_cluster, new_size) { return -1; }
        FD_SIZE[fd] = new_size;
    }

    FD_OFFSET[fd] = offset + len as u32;
    len as i64
}

pub unsafe fn fd_alloc_socket(socket_id: usize) -> i64 {
    for i in 0..MAX_FDS {
        if !FD_USED[i] {
            FD_USED[i] = true;
            FD_KIND[i] = FdKind::Socket;
            FD_SOCKET_ID[i] = socket_id;
            return i as i64;
        }
    }
    -1
}

pub unsafe fn fd_alloc_tcp_socket() -> i64 {
    for i in 0..MAX_FDS {
        if !FD_USED[i] {
            FD_USED[i] = true;
            FD_KIND[i] = FdKind::TcpSocket;
            return i as i64;
        }
    }
    -1
}

pub unsafe fn fd_kind(fd: u64) -> Option<FdKind> {
    if fd as usize >= MAX_FDS || !FD_USED[fd as usize] { return None; }
    Some(FD_KIND[fd as usize])
}

pub unsafe fn fd_socket_id(fd: u64) -> Option<usize> {
    if fd as usize >= MAX_FDS || !FD_USED[fd as usize] { return None; }
    if FD_KIND[fd as usize] != FdKind::Socket { return None; }
    Some(FD_SOCKET_ID[fd as usize])
}