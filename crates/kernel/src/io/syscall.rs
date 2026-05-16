#![allow(dead_code)]

use alloc::string::String;

use super::audit::IoEventKind;

pub struct IoRequest {
    pub kind: IoEventKind,
    pub path: Option<String>,
    pub size: usize,
    pub offset: usize,
}
