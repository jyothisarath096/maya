#![allow(dead_code)]

use alloc::string::String;
use alloc::vec::Vec;

use crate::KernelError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileType {
    Regular,
    Directory,
}

#[derive(Debug, Clone)]
pub struct FileStat {
    pub name: String,
    pub file_type: FileType,
    pub size: usize,
    pub inode: u64,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub file_type: FileType,
    pub inode: u64,
}

pub trait VfsBackend: Send + Sync {
    fn create(&mut self, path: &str, file_type: FileType) -> Result<u64, KernelError>;

    fn open(&self, path: &str) -> Result<u64, KernelError>;

    fn read(&self, inode: u64, offset: usize, buf: &mut [u8]) -> Result<usize, KernelError>;

    fn write(&mut self, inode: u64, offset: usize, data: &[u8]) -> Result<usize, KernelError>;

    fn stat(&self, inode: u64) -> Result<FileStat, KernelError>;

    fn readdir(&self, inode: u64) -> Result<Vec<DirEntry>, KernelError>;

    fn unlink(&mut self, path: &str) -> Result<(), KernelError>;
}
