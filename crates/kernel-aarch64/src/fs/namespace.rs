use crate::{cap::table::RawSpinLock, KernelError};

use super::store::FileId;

pub const MAX_PATH_LEN: usize = 128;
pub const MAX_ENTRIES: usize = 128;

#[derive(Clone, Copy)]
pub struct NamespaceEntry {
    pub valid: bool,
    pub is_dir: bool,
    pub file_id: FileId,
    pub path: [u8; MAX_PATH_LEN],
    pub path_len: usize,
    pub parent_len: usize,
}

impl NamespaceEntry {
    pub const fn empty() -> Self {
        Self {
            valid: false,
            is_dir: false,
            file_id: FileId(0),
            path: [0; MAX_PATH_LEN],
            path_len: 0,
            parent_len: 0,
        }
    }

    pub fn path_matches(&self, path: &[u8]) -> bool {
        self.path_len == path.len() && self.path[..self.path_len] == *path
    }
}

pub struct Namespace {
    entries: [NamespaceEntry; MAX_ENTRIES],
    count: usize,
}

impl Namespace {
    pub const fn new() -> Self {
        Self {
            entries: [NamespaceEntry::empty(); MAX_ENTRIES],
            count: 0,
        }
    }

    pub fn lookup(&self, path: &[u8]) -> Option<&NamespaceEntry> {
        self.entries.iter().find(|e| e.valid && e.path_matches(path))
    }

    pub fn insert(&mut self, path: &[u8], file_id: FileId, is_dir: bool) -> Result<(), KernelError> {
        if self.lookup(path).is_some() {
            return Err(KernelError::InvalidArgument);
        }
        for entry in self.entries.iter_mut() {
            if !entry.valid {
                entry.valid = true;
                entry.is_dir = is_dir;
                entry.file_id = file_id;
                let len = path.len().min(MAX_PATH_LEN);
                entry.path[..len].copy_from_slice(&path[..len]);
                entry.path_len = len;
                entry.parent_len = if len <= 1 {
                    0
                } else {
                    path[..len].iter().rposition(|&b| b == b'/').unwrap_or(0)
                };
                self.count += 1;
                return Ok(());
            }
        }
        Err(KernelError::OutOfMemory)
    }

    pub fn list_dir(&self, dir_path: &[u8], out: &mut [FileId], out_len: &mut usize) {
        *out_len = 0;
        for entry in self.entries.iter() {
            if !entry.valid || entry.is_dir {
                continue;
            }
            let parent = &entry.path[..entry.parent_len];
            if parent == dir_path && *out_len < out.len() {
                out[*out_len] = entry.file_id;
                *out_len += 1;
            }
        }
    }
}

static NAMESPACE: RawSpinLock<Namespace> = RawSpinLock::new(Namespace::new());

pub fn lookup_path(path: &[u8]) -> Option<FileId> {
    let ns = NAMESPACE.lock();
    ns.lookup(path).map(|e| e.file_id)
}

pub fn insert_path(path: &[u8], file_id: FileId, is_dir: bool) -> Result<(), KernelError> {
    NAMESPACE.lock().insert(path, file_id, is_dir)
}

pub fn mkdir(path: &[u8]) -> Result<(), KernelError> {
    insert_path(path, FileId(u32::MAX), true)
}

pub fn list_dir(dir_path: &[u8], out: &mut [FileId]) -> usize {
    let mut len = 0;
    NAMESPACE.lock().list_dir(dir_path, out, &mut len);
    len
}

pub fn snapshot_entries(mut f: impl FnMut(&[u8], FileId, bool)) {
    let ns = NAMESPACE.lock();
    for entry in ns.entries.iter() {
        if !entry.valid {
            continue;
        }
        f(&entry.path[..entry.path_len], entry.file_id, entry.is_dir);
    }
}

pub fn snapshot_entries_try(mut f: impl FnMut(&[u8], FileId, bool)) -> bool {
    let Some(ns) = NAMESPACE.try_lock() else {
        return false;
    };
    for entry in ns.entries.iter() {
        if !entry.valid {
            continue;
        }
        f(&entry.path[..entry.path_len], entry.file_id, entry.is_dir);
    }
    true
}
