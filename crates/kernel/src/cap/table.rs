#![allow(dead_code)]

use alloc::boxed::Box;
use spinning_top::Spinlock;

use crate::KernelError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ResourceType {
    Memory = 0x0001,
    Channel = 0x0002,
    Process = 0x0003,
    Interrupt = 0x0004,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rights(u8);

impl Rights {
    pub const READ: Rights = Rights(0x01);
    pub const WRITE: Rights = Rights(0x02);
    pub const EXECUTE: Rights = Rights(0x04);
    pub const GRANT: Rights = Rights(0x08);
    pub const REVOKE: Rights = Rights(0x10);

    pub fn contains(self, other: Rights) -> bool {
        self.0 & other.0 == other.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapToken(u64);

impl CapToken {
    pub fn from_raw(v: u64) -> CapToken {
        CapToken(v)
    }

    fn generation(self) -> u16 {
        (self.0 >> 48) as u16
    }

    fn owner_pid(self) -> u16 {
        (self.0 >> 32) as u16
    }

    fn resource_type(self) -> u16 {
        (self.0 >> 16) as u16
    }

    fn slot_index(self) -> u16 {
        self.0 as u16
    }
}

#[derive(Debug, Clone)]
pub struct Capability {
    pub token: CapToken,
    pub rights: Rights,
    pub resource_id: u32,
    pub generation: u16,
    pub valid: bool,
}

const TABLE_SIZE: usize = 256;

struct CapTable {
    slots: [Option<Capability>; TABLE_SIZE],
    next_slot: usize,
    generation: [u16; TABLE_SIZE],
}

static CAP_TABLE: Spinlock<Option<Box<CapTable>>> = Spinlock::new(None);

pub fn init() {
    let table = Box::new(CapTable {
        slots: core::array::from_fn(|_| None),
        next_slot: 0,
        generation: [0; TABLE_SIZE],
    });
    *CAP_TABLE.lock() = Some(table);
}

pub fn create(
    owner_pid: u16,
    resource_type: ResourceType,
    resource_id: u32,
    rights: Rights,
) -> Result<CapToken, KernelError> {
    let mut guard = CAP_TABLE.lock();
    let table = guard.as_mut().ok_or(KernelError::CapNotInitialized)?;

    for offset in 0..TABLE_SIZE {
        let slot = (table.next_slot + offset) % TABLE_SIZE;
        if table.slots[slot].is_none() {
            let generation = table.generation[slot];
            let token = pack_token(generation, owner_pid, resource_type as u16, slot as u16);
            table.slots[slot] = Some(Capability {
                token,
                rights,
                resource_id,
                generation,
                valid: true,
            });
            table.next_slot = (slot + 1) % TABLE_SIZE;
            return Ok(token);
        }
    }

    Err(KernelError::CapTableFull)
}

pub fn validate(token: CapToken) -> Result<(), KernelError> {
    let guard = CAP_TABLE.lock();
    let table = guard.as_ref().ok_or(KernelError::CapNotInitialized)?;
    let slot = token.slot_index() as usize;

    if slot >= TABLE_SIZE {
        return Err(KernelError::CapInvalidToken);
    }

    let capability = table.slots[slot]
        .as_ref()
        .ok_or(KernelError::CapInvalidToken)?;

    if !capability.valid
        || token.generation() != table.generation[slot]
        || capability.generation != table.generation[slot]
        || capability.token != token
        || token.owner_pid() != ((token.0 >> 32) as u16)
        || token.resource_type() != ((token.0 >> 16) as u16)
    {
        return Err(KernelError::CapInvalidToken);
    }

    Ok(())
}

pub fn revoke(token: CapToken) -> Result<(), KernelError> {
    validate(token)?;

    let mut guard = CAP_TABLE.lock();
    let table = guard.as_mut().ok_or(KernelError::CapNotInitialized)?;
    let slot = token.slot_index() as usize;
    table.slots[slot] = None;
    table.generation[slot] = table.generation[slot].wrapping_add(1);
    Ok(())
}

pub fn check_right(token: CapToken, right: Rights) -> Result<(), KernelError> {
    validate(token)?;

    let guard = CAP_TABLE.lock();
    let table = guard.as_ref().ok_or(KernelError::CapNotInitialized)?;
    let slot = token.slot_index() as usize;
    let capability = table.slots[slot]
        .as_ref()
        .ok_or(KernelError::CapInvalidToken)?;

    if capability.rights.contains(right) {
        Ok(())
    } else {
        Err(KernelError::CapInsufficientRights)
    }
}

pub fn check_right_as(token: CapToken, right: Rights, caller_pid: u16) -> Result<(), KernelError> {
    validate(token)?;
    if token.owner_pid() != caller_pid {
        return Err(KernelError::CapInvalidToken);
    }
    check_right(token, right)
}

pub fn get_resource_id(token: CapToken) -> Result<u32, KernelError> {
    let guard = CAP_TABLE.lock();
    let table = guard.as_ref().ok_or(KernelError::CapNotInitialized)?;
    let slot = token.slot_index() as usize;
    if slot >= TABLE_SIZE {
        return Err(KernelError::CapInvalidToken);
    }
    let cap = table.slots[slot]
        .as_ref()
        .ok_or(KernelError::CapInvalidToken)?;
    if !cap.valid || token.generation() != table.generation[slot] {
        return Err(KernelError::CapInvalidToken);
    }
    Ok(cap.resource_id)
}

fn pack_token(generation: u16, owner_pid: u16, resource_type: u16, slot: u16) -> CapToken {
    let value = ((generation as u64) << 48)
        | ((owner_pid as u64) << 32)
        | ((resource_type as u64) << 16)
        | (slot as u64);
    CapToken(value)
}
