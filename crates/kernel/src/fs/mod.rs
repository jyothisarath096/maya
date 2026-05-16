#![allow(dead_code)]

use alloc::boxed::Box;
use spinning_top::Spinlock;

use crate::KernelError;

use self::vfs::{DirEntry, FileStat, FileType, VfsBackend};

pub mod memfs;
pub mod path;
pub mod vfs;

static VFS: Spinlock<Option<Box<dyn VfsBackend>>> = Spinlock::new(None);

pub fn init() {
    let fs = Box::new(memfs::MemFs::new());
    *VFS.lock() = Some(fs);
    create("/proc", FileType::Directory).ok();
    create("/dev", FileType::Directory).ok();
    create("/tmp", FileType::Directory).ok();
    crate::serial_print("FS initialised\n");
}

pub fn create(path: &str, file_type: FileType) -> Result<u64, KernelError> {
    let mut guard = VFS.lock();
    let backend = guard.as_mut().ok_or(KernelError::FsNotInitialized)?;
    backend.create(path, file_type)
}

pub fn open(path: &str) -> Result<u64, KernelError> {
    let guard = VFS.lock();
    let backend = guard.as_ref().ok_or(KernelError::FsNotInitialized)?;
    backend.open(path)
}

pub fn read(inode: u64, offset: usize, buf: &mut [u8]) -> Result<usize, KernelError> {
    let guard = VFS.lock();
    let backend = guard.as_ref().ok_or(KernelError::FsNotInitialized)?;
    backend.read(inode, offset, buf)
}

pub fn write(inode: u64, offset: usize, data: &[u8]) -> Result<usize, KernelError> {
    let mut guard = VFS.lock();
    let backend = guard.as_mut().ok_or(KernelError::FsNotInitialized)?;
    backend.write(inode, offset, data)
}

pub fn stat(inode: u64) -> Result<FileStat, KernelError> {
    let guard = VFS.lock();
    let backend = guard.as_ref().ok_or(KernelError::FsNotInitialized)?;
    backend.stat(inode)
}

pub fn readdir(inode: u64) -> Result<alloc::vec::Vec<DirEntry>, KernelError> {
    let guard = VFS.lock();
    let backend = guard.as_ref().ok_or(KernelError::FsNotInitialized)?;
    backend.readdir(inode)
}

pub fn unlink(path: &str) -> Result<(), KernelError> {
    let mut guard = VFS.lock();
    let backend = guard.as_mut().ok_or(KernelError::FsNotInitialized)?;
    backend.unlink(path)
}
