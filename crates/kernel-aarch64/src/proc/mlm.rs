#![allow(dead_code)]

use alloc::vec::Vec;

use crate::{cap::IntentClass, KernelError};

const MLMB_MAGIC: [u8; 4] = *b"MAYA";
const MLMB_VERSION_V2: u32 = 2;
const MLMB_VERSION_V3: u32 = 3;
const MLMB_VERSION_V4: u32 = 4;
const HEADER_LEN_V2: usize = 32;
const HEADER_LEN_V3: usize = 48;
const HEADER_LEN_V4: usize = 56;
const ENTRY_LEN: usize = 48;

#[repr(C, packed)]
pub struct MlmbHeader {
    pub magic: u32,
    pub version: u32,
    pub entry_count: u32,
    pub _pad: u32,
    pub cap_bitmap: u64,
    pub inject_return_vaddr: u64,
    pub shim_load_addr: u64,
    pub guard_addr: u64,
    pub scratch_addr: u64,
}

#[repr(C, packed)]
pub struct MlmbEntryRaw {
    pub intent_id: u64,
    pub entry_vaddr: u64,
    pub name_hash: u64,
    pub intent_class: u16,
    pub cap_rights: u16,
    pub _pad: u32,
    pub name: [u8; 16],
}

#[derive(Clone, Copy)]
pub struct MlmbEntry {
    pub intent_id: u16,
    pub entry_vaddr: u64,
    pub name_hash: u64,
    pub intent_class: IntentClass,
    pub cap_rights: u16,
    pub name: [u8; 16],
}

pub struct ParsedMlmb {
    pub cap_bitmap: u64,
    pub inject_return_vaddr: u64,
    pub shim_load_addr: u64,
    pub guard_addr: u64,
    pub scratch_addr: u64,
    pub entries: Vec<MlmbEntry>,
}

pub fn parse_mlmb(data: &[u8]) -> Option<ParsedMlmb> {
    if data.len() < HEADER_LEN_V2 {
        return None;
    }

    let magic: [u8; 4] = data[0..4].try_into().ok()?;
    let version = u32::from_le_bytes(data[4..8].try_into().ok()?);
    let entry_count = u32::from_le_bytes(data[8..12].try_into().ok()?);
    let cap_bitmap = u64::from_le_bytes(data[16..24].try_into().ok()?);
    let inject_return_vaddr = u64::from_le_bytes(data[24..32].try_into().ok()?);
    let (header_len, shim_load_addr, guard_addr, scratch_addr) = match version {
        MLMB_VERSION_V2 => (HEADER_LEN_V2, 0, 0, 0),
        MLMB_VERSION_V3 => {
            if data.len() < HEADER_LEN_V3 {
                return None;
            }
            let shim_load_addr = u64::from_le_bytes(data[32..40].try_into().ok()?);
            let guard_addr = u64::from_le_bytes(data[40..48].try_into().ok()?);
            (HEADER_LEN_V3, shim_load_addr, guard_addr, 0)
        }
        MLMB_VERSION_V4 => {
            if data.len() < HEADER_LEN_V4 {
                return None;
            }
            let shim_load_addr = u64::from_le_bytes(data[32..40].try_into().ok()?);
            let guard_addr = u64::from_le_bytes(data[40..48].try_into().ok()?);
            let scratch_addr = u64::from_le_bytes(data[48..56].try_into().ok()?);
            (HEADER_LEN_V4, shim_load_addr, guard_addr, scratch_addr)
        }
        _ => return None,
    };

    if magic != MLMB_MAGIC {
        return None;
    }

    let total_len = header_len.checked_add(entry_count as usize * ENTRY_LEN)?;
    if total_len > data.len() {
        return None;
    }

    let mut entries = Vec::with_capacity(entry_count as usize);
    let mut offset = header_len;
    for _ in 0..entry_count as usize {
        let chunk = &data[offset..offset + ENTRY_LEN];
        let intent_id = u64::from_le_bytes(chunk[0..8].try_into().ok()?) as u16;
        let entry_vaddr = u64::from_le_bytes(chunk[8..16].try_into().ok()?);
        let name_hash = u64::from_le_bytes(chunk[16..24].try_into().ok()?);
        let class_raw = u16::from_le_bytes(chunk[24..26].try_into().ok()?);
        let cap_rights = u16::from_le_bytes(chunk[26..28].try_into().ok()?);
        let mut name = [0u8; 16];
        name.copy_from_slice(&chunk[32..48]);

        entries.push(MlmbEntry {
            intent_id,
            entry_vaddr,
            name_hash,
            intent_class: intent_class_from_u16(class_raw),
            cap_rights,
            name,
        });
        offset += ENTRY_LEN;
    }

    Some(ParsedMlmb {
        cap_bitmap,
        inject_return_vaddr,
        shim_load_addr,
        guard_addr,
        scratch_addr,
        entries,
    })
}

pub fn validate_mlmb(data: &[u8]) -> Result<(), KernelError> {
    parse_mlmb(data).map(|_| ()).ok_or(KernelError::InvalidElf)
}

fn intent_class_from_u16(value: u16) -> IntentClass {
    match value {
        1 => IntentClass::Compute,
        2 => IntentClass::IO,
        3 => IntentClass::RealTime,
        4 => IntentClass::Background,
        5 => IntentClass::System,
        _ => IntentClass::Unknown,
    }
}
