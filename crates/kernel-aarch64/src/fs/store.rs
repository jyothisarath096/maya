extern crate alloc;

use alloc::vec::Vec;

use crate::{cap::table::RawSpinLock, KernelError};

pub const MAX_FILES: usize = 64;
pub const MAX_FILE_SIZE: usize = 65536;
pub const MAX_VERSIONS: usize = 8;
pub const MAX_TAGS: usize = 4;
pub const MAX_TAG_KEY_LEN: usize = 16;
pub const MAX_TAG_VAL_LEN: usize = 32;

pub const VFILE_PROC_STATS: u32 = 0x0001;
pub const VFILE_PROC_INTENT: u32 = 0x0002;
pub const VFILE_PROC_NAME: u32 = 0x0003;
pub const VFILE_SCHED: u32 = 0x0004;
pub const VFILE_FS_INFO: u32 = 0x0005;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FileId(pub u32);

#[derive(Clone)]
pub struct FileVersion {
    pub data: Vec<u8>,
    pub size: usize,
    pub content_hash: u64,
    pub written_ns: u64,
    pub written_by: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum FileIntent {
    Read = 0,
    Write = 1,
    Execute = 2,
    Index = 3,
    Transfer = 4,
    Backup = 5,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Regular,
    Virtual,
}

#[derive(Clone, Copy)]
pub struct SemanticTag {
    pub valid: bool,
    pub key: [u8; MAX_TAG_KEY_LEN],
    pub key_len: usize,
    pub val: [u8; MAX_TAG_VAL_LEN],
    pub val_len: usize,
}

impl SemanticTag {
    pub const fn empty() -> Self {
        Self {
            valid: false,
            key: [0; MAX_TAG_KEY_LEN],
            key_len: 0,
            val: [0; MAX_TAG_VAL_LEN],
            val_len: 0,
        }
    }
}

pub struct FileEntry {
    pub valid: bool,
    pub file_id: FileId,
    pub kind: FileKind,
    pub virtual_id: u32,
    pub intent_class: u8,
    pub created_ns: u64,
    pub creator_pid: u16,
    pub versions: [Option<FileVersion>; MAX_VERSIONS],
    pub version_count: u32,
    pub current_version: u32,
    pub access_count: u64,
    pub last_access_ns: u64,
    pub last_accessor_pid: u16,
    pub tags: [SemanticTag; MAX_TAGS],
    pub tag_count: u8,
}

impl FileEntry {
    pub const fn empty() -> Self {
        Self {
            valid: false,
            file_id: FileId(0),
            kind: FileKind::Regular,
            virtual_id: 0,
            intent_class: 0,
            created_ns: 0,
            creator_pid: 0,
            versions: [const { None }; MAX_VERSIONS],
            version_count: 0,
            current_version: 0,
            access_count: 0,
            last_access_ns: 0,
            last_accessor_pid: 0,
            tags: [const { SemanticTag::empty() }; MAX_TAGS],
            tag_count: 0,
        }
    }

    pub fn current_data(&self) -> Option<&FileVersion> {
        let idx = self.current_version as usize % MAX_VERSIONS;
        self.versions[idx].as_ref()
    }
}

pub fn content_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub struct FileStore {
    pub entries: [FileEntry; MAX_FILES],
    pub count: usize,
}

impl FileStore {
    pub const fn new() -> Self {
        Self {
            entries: [const { FileEntry::empty() }; MAX_FILES],
            count: 0,
        }
    }

    pub fn alloc(&mut self, creator_pid: u16, intent_class: u8, now_ns: u64) -> Option<FileId> {
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if !entry.valid {
                let fid = FileId(i as u32);
                *entry = FileEntry::empty();
                entry.valid = true;
                entry.file_id = fid;
                entry.kind = FileKind::Regular;
                entry.creator_pid = creator_pid;
                entry.intent_class = intent_class;
                entry.created_ns = now_ns;
                self.count += 1;
                return Some(fid);
            }
        }
        None
    }

    pub fn write(
        &mut self,
        fid: FileId,
        data: &[u8],
        pid: u16,
        now_ns: u64,
    ) -> Result<u32, KernelError> {
        let entry = self
            .entries
            .get_mut(fid.0 as usize)
            .filter(|e| e.valid)
            .ok_or(KernelError::InvalidArgument)?;
        let size = data.len().min(MAX_FILE_SIZE);
        let hash = content_hash(&data[..size]);
        let version = FileVersion {
            data: data[..size].to_vec(),
            size,
            content_hash: hash,
            written_ns: now_ns,
            written_by: pid,
        };
        let version_num = entry.version_count;
        let idx = version_num as usize % MAX_VERSIONS;
        entry.versions[idx] = Some(version);
        entry.version_count = entry.version_count.saturating_add(1);
        entry.current_version = version_num;
        entry.access_count = entry.access_count.saturating_add(1);
        entry.last_access_ns = now_ns;
        entry.last_accessor_pid = pid;
        Ok(version_num)
    }

    pub fn read(&mut self, fid: FileId, pid: u16, now_ns: u64) -> Result<&FileVersion, KernelError> {
        let entry = self
            .entries
            .get_mut(fid.0 as usize)
            .filter(|e| e.valid)
            .ok_or(KernelError::InvalidArgument)?;
        entry.access_count = entry.access_count.saturating_add(1);
        entry.last_access_ns = now_ns;
        entry.last_accessor_pid = pid;
        entry.current_data().ok_or(KernelError::InvalidArgument)
    }

    pub fn stat(&self, fid: FileId) -> Result<(usize, u32), KernelError> {
        let entry = self
            .entries
            .get(fid.0 as usize)
            .filter(|e| e.valid)
            .ok_or(KernelError::InvalidArgument)?;
        let size = match entry.kind {
            FileKind::Regular => entry.current_data().map(|v| v.size).unwrap_or(0),
            FileKind::Virtual => 256,
        };
        Ok((size, entry.version_count))
    }

    fn alloc_virtual_inner(
        &mut self,
        creator_pid: u16,
        virtual_id: u32,
        now_ns: u64,
    ) -> Option<FileId> {
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if !entry.valid {
                let fid = FileId(i as u32);
                *entry = FileEntry::empty();
                entry.valid = true;
                entry.file_id = fid;
                entry.kind = FileKind::Virtual;
                entry.virtual_id = virtual_id;
                entry.creator_pid = creator_pid;
                entry.created_ns = now_ns;
                self.count += 1;
                return Some(fid);
            }
        }
        None
    }

    pub fn add_tag(&mut self, fid: FileId, key: &[u8], val: &[u8]) -> Result<(), KernelError> {
        let entry = self
            .entries
            .get_mut(fid.0 as usize)
            .filter(|e| e.valid)
            .ok_or(KernelError::InvalidArgument)?;

        for tag in entry.tags.iter_mut() {
            if tag.valid && tag.key_len == key.len() && tag.key[..tag.key_len] == *key {
                let vlen = val.len().min(MAX_TAG_VAL_LEN);
                tag.val[..vlen].copy_from_slice(&val[..vlen]);
                tag.val_len = vlen;
                return Ok(());
            }
        }

        for tag in entry.tags.iter_mut() {
            if !tag.valid {
                let klen = key.len().min(MAX_TAG_KEY_LEN);
                let vlen = val.len().min(MAX_TAG_VAL_LEN);
                tag.key[..klen].copy_from_slice(&key[..klen]);
                tag.key_len = klen;
                tag.val[..vlen].copy_from_slice(&val[..vlen]);
                tag.val_len = vlen;
                tag.valid = true;
                entry.tag_count = entry.tag_count.saturating_add(1);
                return Ok(());
            }
        }

        Err(KernelError::OutOfMemory)
    }

    pub fn query_by_tag(&self, key: &[u8], val: &[u8], out: &mut [FileId]) -> usize {
        let mut count = 0usize;
        for entry in self.entries.iter() {
            if !entry.valid || count >= out.len() {
                continue;
            }
            for tag in entry.tags.iter() {
                if !tag.valid {
                    continue;
                }
                if tag.key_len == key.len()
                    && tag.key[..tag.key_len] == *key
                    && (val.is_empty()
                        || (tag.val_len == val.len() && tag.val[..tag.val_len] == *val))
                {
                    out[count] = entry.file_id;
                    count += 1;
                    break;
                }
            }
        }
        count
    }

    pub fn query_by_intent(&self, intent_class: u8, out: &mut [FileId]) -> usize {
        let mut count = 0usize;
        for entry in self.entries.iter() {
            if !entry.valid || entry.kind != FileKind::Regular || count >= out.len() {
                continue;
            }
            if intent_class == 0 || entry.intent_class == intent_class {
                out[count] = entry.file_id;
                count += 1;
            }
        }
        count
    }
}

static FILE_STORE: RawSpinLock<FileStore> = RawSpinLock::new(FileStore::new());

pub fn alloc_file(creator_pid: u16, intent_class: u8, now_ns: u64) -> Option<FileId> {
    FILE_STORE.lock().alloc(creator_pid, intent_class, now_ns)
}

pub fn alloc_virtual(creator_pid: u16, virtual_id: u32, now_ns: u64) -> Option<FileId> {
    FILE_STORE
        .lock()
        .alloc_virtual_inner(creator_pid, virtual_id, now_ns)
}

pub fn write_file(fid: FileId, data: &[u8], pid: u16, now_ns: u64) -> Result<u32, KernelError> {
    let version = FILE_STORE.lock().write(fid, data, pid, now_ns)?;
    let _ = crate::sched::queue::update_process_file_stats(pid, 0, 1, data.len().min(MAX_FILE_SIZE) as u64);
    Ok(version)
}

pub fn read_file_copy(
    fid: FileId,
    pid: u16,
    now_ns: u64,
    out: &mut [u8],
) -> Result<usize, KernelError> {
    let mut store = FILE_STORE.lock();
    let version = store.read(fid, pid, now_ns)?;
    let n = version.size.min(out.len());
    out[..n].copy_from_slice(&version.data[..n]);
    drop(store);
    let _ = crate::sched::queue::update_process_file_stats(pid, 1, 0, 0);
    Ok(n)
}

pub fn file_exists(fid: FileId) -> bool {
    FILE_STORE
        .lock()
        .entries
        .get(fid.0 as usize)
        .map(|e| e.valid)
        .unwrap_or(false)
}

pub fn stat_file(fid: FileId) -> Result<(usize, u32), KernelError> {
    FILE_STORE.lock().stat(fid)
}

pub fn read_file_version(
    fid: FileId,
    version_num: u32,
    pid: u16,
    now_ns: u64,
    out: &mut [u8],
) -> Result<(usize, u32), KernelError> {
    FILE_STORE
        .lock()
        .read_version(fid, version_num, pid, now_ns, out)
}

pub fn file_version_count(fid: FileId) -> u32 {
    let store = FILE_STORE.lock();
    store
        .entries
        .get(fid.0 as usize)
        .filter(|e| e.valid)
        .map(|e| e.version_count)
        .unwrap_or(0)
}

pub fn file_oldest_version(fid: FileId) -> u32 {
    let store = FILE_STORE.lock();
    store
        .entries
        .get(fid.0 as usize)
        .filter(|e| e.valid)
        .map(|e| e.version_count.saturating_sub(MAX_VERSIONS as u32))
        .unwrap_or(0)
}

pub fn file_is_active(fid: FileId) -> bool {
    let store = FILE_STORE.lock();
    store
        .entries
        .get(fid.0 as usize)
        .filter(|e| e.valid)
        .map(|e| e.access_count > 0 || e.version_count > 0)
        .unwrap_or(false)
}

pub fn file_metadata_try(fid: FileId) -> Option<(u32, bool)> {
    let store = FILE_STORE.try_lock()?;
    store
        .entries
        .get(fid.0 as usize)
        .filter(|e| e.valid)
        .map(|e| (e.version_count, e.access_count > 0 || e.version_count > 0))
}

pub fn is_virtual_file(fid: FileId) -> bool {
    FILE_STORE
        .lock()
        .entries
        .get(fid.0 as usize)
        .map(|e| e.valid && e.kind == FileKind::Virtual)
        .unwrap_or(false)
}

pub fn tag_file(fid: FileId, key: &[u8], val: &[u8]) -> Result<(), KernelError> {
    FILE_STORE.lock().add_tag(fid, key, val)
}

pub fn query_files_by_tag(key: &[u8], val: &[u8], out: &mut [FileId]) -> usize {
    FILE_STORE.lock().query_by_tag(key, val, out)
}

pub fn query_files_by_intent(intent: u8, out: &mut [FileId]) -> usize {
    FILE_STORE.lock().query_by_intent(intent, out)
}

impl FileStore {
    pub fn read_version(
        &mut self,
        fid: FileId,
        version_num: u32,
        pid: u16,
        now_ns: u64,
        out: &mut [u8],
    ) -> Result<(usize, u32), KernelError> {
        let entry = self
            .entries
            .get_mut(fid.0 as usize)
            .filter(|e| e.valid && e.kind == FileKind::Regular)
            .ok_or(KernelError::InvalidArgument)?;

        let actual_version = if version_num == u32::MAX {
            if entry.version_count == 0 {
                return Err(KernelError::InvalidArgument);
            }
            entry.current_version
        } else {
            version_num
        };

        if entry.version_count == 0 {
            return Err(KernelError::InvalidArgument);
        }
        let oldest_available = entry.version_count.saturating_sub(MAX_VERSIONS as u32);
        if actual_version < oldest_available || actual_version >= entry.version_count {
            return Err(KernelError::InvalidArgument);
        }

        let idx = actual_version as usize % MAX_VERSIONS;
        let version = entry.versions[idx]
            .as_ref()
            .ok_or(KernelError::InvalidArgument)?;
        let n = version.size.min(out.len());
        out[..n].copy_from_slice(&version.data[..n]);

        entry.access_count = entry.access_count.saturating_add(1);
        entry.last_access_ns = now_ns;
        entry.last_accessor_pid = pid;

        Ok((n, actual_version))
    }
}

pub fn read_virtual(fid: FileId, _pid: u16, out: &mut [u8]) -> Result<usize, KernelError> {
    let store = FILE_STORE.lock();
    let entry = store
        .entries
        .get(fid.0 as usize)
        .filter(|e| e.valid && e.kind == FileKind::Virtual)
        .ok_or(KernelError::InvalidArgument)?;
    let vid = entry.virtual_id;
    let target_pid = (vid >> 16) as u16;
    let vtype = vid & 0xFFFF;
    drop(store);
    generate_virtual(vtype, target_pid, out)
}

fn generate_virtual(vtype: u32, target_pid: u16, out: &mut [u8]) -> Result<usize, KernelError> {
    match vtype {
        VFILE_PROC_STATS => generate_proc_stats(target_pid, out),
        VFILE_PROC_INTENT => generate_proc_intent(target_pid, out),
        VFILE_PROC_NAME => generate_proc_name(target_pid, out),
        VFILE_SCHED => generate_sched_info(out),
        VFILE_FS_INFO => generate_fs_info(out),
        _ => Err(KernelError::InvalidArgument),
    }
}

fn generate_proc_stats(pid: u16, out: &mut [u8]) -> Result<usize, KernelError> {
    let process = crate::sched::queue::get_process(pid).ok_or(KernelError::InvalidArgument)?;
    let mut buf = [0u8; 256];
    let mut pos = 0;
    pos += write_field(&mut buf[pos..], b"cpu_ticks:", process.stats.cpu_ticks_used);
    pos += write_field(&mut buf[pos..], b"ipc_sends:", process.stats.ipc_sends);
    pos += write_field(&mut buf[pos..], b"ipc_recvs:", process.stats.ipc_recvs);
    pos += write_field(&mut buf[pos..], b"file_reads:", process.stats.file_reads);
    pos += write_field(&mut buf[pos..], b"file_writes:", process.stats.file_writes);
    pos += write_field(&mut buf[pos..], b"intent_fires:", process.stats.intent_fire_count);
    let n = pos.min(out.len());
    out[..n].copy_from_slice(&buf[..n]);
    Ok(n)
}

fn generate_proc_intent(pid: u16, out: &mut [u8]) -> Result<usize, KernelError> {
    let process = crate::sched::queue::get_process(pid).ok_or(KernelError::InvalidArgument)?;
    let mut buf = [0u8; 64];
    let mut pos = 0;
    pos += write_field(&mut buf[pos..], b"intent_id:", process.stats.last_intent_id as u64);
    pos += write_field(&mut buf[pos..], b"intent_fires:", process.stats.intent_fire_count);
    let n = pos.min(out.len());
    out[..n].copy_from_slice(&buf[..n]);
    Ok(n)
}

fn generate_proc_name(pid: u16, out: &mut [u8]) -> Result<usize, KernelError> {
    let name = crate::proc::get_process_name(pid).ok_or(KernelError::InvalidArgument)?;
    let n = name.len().min(out.len());
    out[..n].copy_from_slice(&name[..n]);
    Ok(n)
}

fn generate_sched_info(out: &mut [u8]) -> Result<usize, KernelError> {
    let model = crate::model::weights::load();
    let sum: i32 = model.out_w.iter().map(|&w| w as i32).sum();
    let mut buf = [0u8; 64];
    let mut pos = 0;
    pos += write_field(&mut buf[pos..], b"out_w_sum:", sum as u64);
    let n = pos.min(out.len());
    out[..n].copy_from_slice(&buf[..n]);
    Ok(n)
}

fn generate_fs_info(out: &mut [u8]) -> Result<usize, KernelError> {
    let store = FILE_STORE.lock();
    let count = store.count;
    drop(store);
    let mut buf = [0u8; 64];
    let n = write_field(&mut buf, b"files:", count as u64).min(out.len());
    out[..n].copy_from_slice(&buf[..n]);
    Ok(n)
}

fn write_field(buf: &mut [u8], key: &[u8], val: u64) -> usize {
    let mut pos = 0;
    let klen = key.len().min(buf.len());
    buf[..klen].copy_from_slice(&key[..klen]);
    pos += klen;
    let mut tmp = [0u8; 20];
    let mut tlen = 0usize;
    let mut v = val;
    if v == 0 {
        tmp[0] = b'0';
        tlen = 1;
    } else {
        while v > 0 && tlen < tmp.len() {
            tmp[tlen] = b'0' + (v % 10) as u8;
            v /= 10;
            tlen += 1;
        }
        tmp[..tlen].reverse();
    }
    let copy = tlen.min(buf.len().saturating_sub(pos));
    buf[pos..pos + copy].copy_from_slice(&tmp[..copy]);
    pos += copy;
    if pos < buf.len() {
        buf[pos] = b'\n';
        pos += 1;
    }
    pos
}
