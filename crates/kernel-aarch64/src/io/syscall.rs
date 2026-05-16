#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoEventKind {
    FileOpen,
    FileRead,
    FileWrite,
    FileCreate,
    FileUnlink,
    NetworkSend,
    NetworkRecv,
    MemoryMap,
}

#[derive(Debug, Clone, Copy)]
pub struct IoRequest {
    pub kind: IoEventKind,
    pub path: Option<[u8; 64]>,
    pub path_len: usize,
    pub size: usize,
    pub offset: usize,
    pub cap_token: Option<crate::cap::CapToken>,
}

impl IoRequest {
    pub fn path_str(&self) -> Option<&str> {
        self.path
            .as_ref()
            .and_then(|p| core::str::from_utf8(&p[..self.path_len]).ok())
    }
}
