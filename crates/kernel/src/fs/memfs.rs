#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::{
    path,
    vfs::{DirEntry, FileStat, FileType, VfsBackend},
};
use crate::KernelError;

struct INode {
    inode: u64,
    file_type: FileType,
    data: Vec<u8>,
    children: BTreeMap<String, u64>,
    name: String,
}

pub struct MemFs {
    inodes: BTreeMap<u64, INode>,
    path_index: BTreeMap<String, u64>,
    next_inode: u64,
}

impl MemFs {
    pub fn new() -> Self {
        let mut fs = MemFs {
            inodes: BTreeMap::new(),
            path_index: BTreeMap::new(),
            next_inode: 2,
        };
        let root = INode {
            inode: 1,
            file_type: FileType::Directory,
            data: Vec::new(),
            children: BTreeMap::new(),
            name: String::from("/"),
        };
        fs.inodes.insert(1, root);
        fs.path_index.insert(String::from("/"), 1);
        fs
    }
}

impl VfsBackend for MemFs {
    fn create(&mut self, path: &str, file_type: FileType) -> Result<u64, KernelError> {
        let path = path::normalize(path);
        if path == "/" || self.path_index.contains_key(&path) {
            return Err(KernelError::FsAlreadyExists);
        }

        let parent_path = path::parent(&path);
        let name = path::filename(&path);
        if name.is_empty() {
            return Err(KernelError::FsInvalidPath);
        }

        let parent_inode_num = *self
            .path_index
            .get(parent_path)
            .ok_or(KernelError::FsFileNotFound)?;
        let parent_inode = self
            .inodes
            .get_mut(&parent_inode_num)
            .ok_or(KernelError::FsFileNotFound)?;
        if parent_inode.file_type != FileType::Directory {
            return Err(KernelError::FsNotADirectory);
        }

        let inode_num = self.next_inode;
        self.next_inode += 1;
        parent_inode.children.insert(String::from(name), inode_num);
        self.inodes.insert(
            inode_num,
            INode {
                inode: inode_num,
                file_type: file_type.clone(),
                data: Vec::new(),
                children: BTreeMap::new(),
                name: String::from(name),
            },
        );
        self.path_index.insert(path, inode_num);
        Ok(inode_num)
    }

    fn open(&self, path: &str) -> Result<u64, KernelError> {
        let path = path::normalize(path);
        self.path_index
            .get(&path)
            .copied()
            .ok_or(KernelError::FsFileNotFound)
    }

    fn read(&self, inode: u64, offset: usize, buf: &mut [u8]) -> Result<usize, KernelError> {
        let inode = self.inodes.get(&inode).ok_or(KernelError::FsFileNotFound)?;
        if inode.file_type != FileType::Regular {
            return Err(KernelError::FsNotAFile);
        }

        if offset >= inode.data.len() {
            return Ok(0);
        }

        let available = inode.data.len() - offset;
        let count = available.min(buf.len());
        buf[..count].copy_from_slice(&inode.data[offset..offset + count]);
        Ok(count)
    }

    fn write(&mut self, inode: u64, offset: usize, data: &[u8]) -> Result<usize, KernelError> {
        let inode = self
            .inodes
            .get_mut(&inode)
            .ok_or(KernelError::FsFileNotFound)?;
        if inode.file_type != FileType::Regular {
            return Err(KernelError::FsNotAFile);
        }

        let required = offset.saturating_add(data.len());
        if inode.data.len() < required {
            inode.data.resize(required, 0);
        }
        inode.data[offset..offset + data.len()].copy_from_slice(data);
        Ok(data.len())
    }

    fn stat(&self, inode: u64) -> Result<FileStat, KernelError> {
        let inode = self.inodes.get(&inode).ok_or(KernelError::FsFileNotFound)?;
        Ok(FileStat {
            name: inode.name.clone(),
            file_type: inode.file_type.clone(),
            size: inode.data.len(),
            inode: inode.inode,
        })
    }

    fn readdir(&self, inode: u64) -> Result<Vec<DirEntry>, KernelError> {
        let inode = self.inodes.get(&inode).ok_or(KernelError::FsFileNotFound)?;
        if inode.file_type != FileType::Directory {
            return Err(KernelError::FsNotADirectory);
        }

        let mut entries = Vec::new();
        for (name, child_inode) in &inode.children {
            let child = self
                .inodes
                .get(child_inode)
                .ok_or(KernelError::FsFileNotFound)?;
            entries.push(DirEntry {
                name: name.clone(),
                file_type: child.file_type.clone(),
                inode: *child_inode,
            });
        }
        Ok(entries)
    }

    fn unlink(&mut self, path: &str) -> Result<(), KernelError> {
        let path = path::normalize(path);
        if path == "/" {
            return Err(KernelError::FsPermissionDenied);
        }

        let inode_num = self
            .path_index
            .get(&path)
            .copied()
            .ok_or(KernelError::FsFileNotFound)?;
        let parent_path = path::parent(&path);
        let name = path::filename(&path);

        let inode = self
            .inodes
            .get(&inode_num)
            .ok_or(KernelError::FsFileNotFound)?;
        if inode.file_type == FileType::Directory && !inode.children.is_empty() {
            return Err(KernelError::FsPermissionDenied);
        }

        let parent_inode_num = self
            .path_index
            .get(parent_path)
            .copied()
            .ok_or(KernelError::FsFileNotFound)?;
        let parent_inode = self
            .inodes
            .get_mut(&parent_inode_num)
            .ok_or(KernelError::FsFileNotFound)?;
        parent_inode.children.remove(name);

        self.path_index.remove(&path);
        self.inodes.remove(&inode_num);
        Ok(())
    }
}
