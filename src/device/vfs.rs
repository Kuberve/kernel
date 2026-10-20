/// Virtual File System: абстракция над конкретной ФС.
/// Пока минимально — только то, что нужно для mount + parse.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    NotFound,
    NotADirectory,
    NotAFile,
    IoError,
    InvalidFs,
    EndOfFile,
    OutOfBounds,
}

pub type VfsResult<T> = Result<T, VfsError>;

/// Файл в VFS. Пока только метаданные, без буфера.
#[allow(dead_code)]
pub struct VfsFile {
    pub inode_id: u64,      // для FAT32 — first cluster
    pub size: u64,
    pub is_dir: bool,
    pub offset: u64,
    pub first_cluster: u64,
}

/// Информация о смонтированной ФС.
#[allow(dead_code)]
pub struct MountInfo {
    pub fs_type: &'static str,
    pub label: [u8; 11],
    pub cluster_size: u32,
    pub total_clusters: u32,
    pub root_cluster: u32,
}