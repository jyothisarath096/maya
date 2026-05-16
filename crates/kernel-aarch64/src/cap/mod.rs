pub mod cache;
pub mod fuzz;
pub mod table;

use core::sync::atomic::{AtomicBool, Ordering};

use crate::KernelError;
use table::{RawSpinLock, RawSpinLockGuard};

const TABLE_SIZE: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct CapToken(pub u128);

impl CapToken {
    pub fn generation(self) -> u32 {
        (self.0 >> 96) as u32
    }
    pub fn owner_pid(self) -> u16 {
        (self.0 >> 80) as u16
    }
    pub fn rights(self) -> u16 {
        (self.0 >> 64) as u16
    }
    pub fn resource_type(self) -> u16 {
        (self.0 >> 48) as u16
    }
    pub fn intent_id(self) -> u16 {
        (self.0 >> 32) as u16
    }
    pub fn slot_index(self) -> u32 {
        self.0 as u32
    }
    pub fn hi(self) -> u64 {
        (self.0 >> 64) as u64
    }
    pub fn lo(self) -> u64 {
        self.0 as u64
    }
    pub fn from_raw(v: u128) -> Self {
        CapToken(v)
    }
    pub fn from_parts(hi: u64, lo: u64) -> Self {
        CapToken(((hi as u128) << 64) | lo as u128)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ResourceType {
    Memory = 0x0001,
    Channel = 0x0002,
    Process = 0x0003,
    Interrupt = 0x0004,
    Intent = 0x0005,
    Telemetry = 0x0006,
    Network = 0x0007,
    Crypto = 0x0008,
    Console = 0x0009,
    File = 0x000A,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rights(pub u16);

impl Rights {
    pub const READ: Rights = Rights(0x0001);
    pub const WRITE: Rights = Rights(0x0002);
    pub const EXECUTE: Rights = Rights(0x0004);
    pub const GRANT: Rights = Rights(0x0008);
    pub const REVOKE: Rights = Rights(0x0010);
    pub const INTENT_CALL: Rights = Rights(0x0020);
    pub const OBSERVE: Rights = Rights(0x0040);

    pub fn contains(self, other: Rights) -> bool {
        self.0 & other.0 == other.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum IntentClass {
    Unknown = 0x0000,
    Compute = 0x0001,
    IO = 0x0002,
    RealTime = 0x0003,
    Background = 0x0004,
    System = 0x0005,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Capability {
    pub token: CapToken,
    pub resource_id: u32,
    pub generation: u32,
    pub valid: bool,
    pub intent_class: IntentClass,
    pub created_tick: u64,
    pub last_used_tick: u64,
    pub parent_slot: Option<u32>,
    pub delegation_depth: u8,
    pub max_depth: u8,
}

struct CapTable {
    slots: [Option<Capability>; TABLE_SIZE],
    generation: [u32; TABLE_SIZE],
    next_slot: usize,
}

static CAP_TABLE: RawSpinLock<CapTable> = RawSpinLock::new(CapTable {
    slots: [None; TABLE_SIZE],
    generation: [0; TABLE_SIZE],
    next_slot: 0,
});

static PAC_AVAILABLE: AtomicBool = AtomicBool::new(false);
static MTE_AVAILABLE: AtomicBool = AtomicBool::new(false);
static BTI_AVAILABLE: AtomicBool = AtomicBool::new(false);

macro_rules! bti_c {
    () => {
        unsafe {
            core::arch::asm!(
                ".inst 0xD503245F",
                options(nostack, nomem, preserves_flags)
            );
        }
    };
}

pub fn pac_available() -> bool {
    PAC_AVAILABLE.load(Ordering::Acquire)
}

pub fn mte_available() -> bool {
    MTE_AVAILABLE.load(Ordering::Acquire)
}

pub fn init() {
    bti_c!();
    unsafe {
        core::arch::asm!(
            ".inst 0xD500409F",
            options(nostack, nomem, preserves_flags)
        );
    }

    let isar1: u64;
    let pfr1: u64;
    unsafe {
        core::arch::asm!("mrs {r}, id_aa64isar1_el1", r = out(reg) isar1, options(nomem, nostack));
        core::arch::asm!("mrs {r}, id_aa64pfr1_el1", r = out(reg) pfr1, options(nomem, nostack));
    }
    let apa = ((isar1 >> 4) & 0xF) as u8;
    let gpa = ((isar1 >> 24) & 0xF) as u8;
    let _mte = ((pfr1 >> 8) & 0xF) as u8;
    let bt = (pfr1 & 0xF) as u8;

    PAC_AVAILABLE.store(apa != 0 || gpa != 0, Ordering::Release);
    MTE_AVAILABLE.store(false, Ordering::Release);
    BTI_AVAILABLE.store(bt != 0, Ordering::Release);

    if !pac_available() {
        crate::uart_print!("PAC not available\n");
    }
    if !mte_available() {
        crate::uart_print!("MTE: unavailable (QEMU)\n");
    }
    if !BTI_AVAILABLE.load(Ordering::Acquire) {
        crate::uart_print!("BTI not available\n");
    }

    let mut table = CAP_TABLE.lock();
    table.next_slot = 0;
    for i in 0..TABLE_SIZE {
        table.slots[i] = None;
        table.generation[i] = 0;
    }
}

fn pack_token(
    generation: u32,
    owner_pid: u16,
    rights: Rights,
    resource_type: ResourceType,
    intent_id: u16,
    slot_index: u32,
) -> Result<CapToken, KernelError> {
    let hi = ((generation as u64) << 32)
        | ((owner_pid as u64) << 16)
        | (rights.0 as u64);
    let lo = ((resource_type as u64) << 48) | ((intent_id as u64) << 32) | slot_index as u64;
    let signed_hi = if pac_available() { sign_hi(hi, lo)? } else { hi };
    Ok(CapToken::from_parts(signed_hi, lo))
}

fn sign_hi(hi: u64, lo: u64) -> Result<u64, KernelError> {
    let mut out = hi;
    unsafe {
        core::arch::asm!(
            "pacda {out}, {modifier}",
            out = inout(reg) out,
            modifier = in(reg) lo,
            options(nomem, nostack)
        );
    }
    Ok(out)
}

pub(crate) fn auth_token(token: CapToken) -> Result<CapToken, KernelError> {
    if !pac_available() {
        return Ok(token);
    }
    let mut hi = token.hi();
    let lo = token.lo();
    unsafe {
        core::arch::asm!(
            "autda {hi}, {lo}",
            hi = inout(reg) hi,
            lo = in(reg) lo,
            options(nomem, nostack)
        );
    }
    if ((hi >> 55) & 1) != 0 {
        return Err(KernelError::CapInvalidToken);
    }
    Ok(CapToken::from_parts(hi, lo))
}

fn maybe_tag_slot(_slot_ptr: *mut Capability, _generation: u32) {
    // MTE disabled: QEMU TCG does not implement MTE memory instructions.
    // Real hardware path: enable via feature flag when running on
    // ARMv8.5+ silicon (Cortex-A55+, Apple M1+, Graviton3+).
}

fn locked_validate<'a>(
    table: &mut RawSpinLockGuard<'a, CapTable>,
    token: CapToken,
) -> Result<(usize, CapToken), KernelError> {
    let authed = auth_token(token)?;
    let slot = authed.slot_index() as usize;
    if slot >= TABLE_SIZE {
        unsafe {
            core::arch::asm!("csdb", options(nomem, nostack, preserves_flags));
        }
        return Err(KernelError::CapInvalidToken);
    }
    let Some(mut cap) = table.slots[slot] else {
        unsafe {
            core::arch::asm!("csdb", options(nomem, nostack, preserves_flags));
        }
        return Err(KernelError::CapInvalidToken);
    };
    if !cap.valid {
        unsafe {
            core::arch::asm!("csdb", options(nomem, nostack, preserves_flags));
        }
        return Err(KernelError::CapInvalidToken);
    }
    if table.generation[slot] != authed.generation() || cap.generation != authed.generation() {
        unsafe {
            core::arch::asm!("csdb", options(nomem, nostack, preserves_flags));
        }
        return Err(KernelError::CapInvalidToken);
    }
    cap.last_used_tick = crate::arch::timer::current_tick();
    table.slots[slot] = Some(cap);
    Ok((slot, authed))
}

pub fn create(
    owner_pid: u16,
    resource_type: ResourceType,
    resource_id: u32,
    rights: Rights,
    intent_id: u16,
    intent_class: IntentClass,
) -> Result<CapToken, KernelError> {
    bti_c!();
    let mut table = CAP_TABLE.lock();
    for scan in 0..TABLE_SIZE {
        let slot = (table.next_slot + scan) % TABLE_SIZE;
        if table.slots[slot].is_none() {
            let generation = table.generation[slot];
            let token = pack_token(
                generation,
                owner_pid,
                rights,
                resource_type,
                intent_id,
                slot as u32,
            )?;
            let now = crate::arch::timer::current_tick();
            let cap = Capability {
                token,
                resource_id,
                generation,
                valid: true,
                intent_class,
                created_tick: now,
                last_used_tick: now,
                parent_slot: None,
                delegation_depth: 0,
                max_depth: 3,
            };
            table.slots[slot] = Some(cap);
            table.next_slot = (slot + 1) % TABLE_SIZE;
            if let Some(stored) = &mut table.slots[slot] {
                maybe_tag_slot(stored as *mut Capability, generation);
            }
            return Ok(token);
        }
    }
    Err(KernelError::CapTableFull)
}

pub fn validate(token: CapToken) -> Result<(), KernelError> {
    bti_c!();
    if cache::cache_lookup(token).is_some() {
        return Ok(());
    }
    let mut table = CAP_TABLE.lock();
    let (_slot, authed) = locked_validate(&mut table, token)?;
    cache::cache_insert(authed);
    Ok(())
}

pub fn revoke(token: CapToken) -> Result<(), KernelError> {
    bti_c!();
    let mut table = CAP_TABLE.lock();
    let (slot, _authed) = locked_validate(&mut table, token)?;
    let Some(cap) = table.slots[slot].as_ref() else {
        return Err(KernelError::CapInvalidToken);
    };
    let expected = cap.generation;
    let new_generation = expected.wrapping_add(1);
    unsafe {
        let gen_ptr = core::ptr::addr_of_mut!(table.generation[slot]);
        let mut old = expected;
        core::arch::asm!(
            "cas {old:w}, {new:w}, [{ptr}]",
            old = inout(reg) old,
            new = in(reg) new_generation,
            ptr = in(reg) gen_ptr,
            options(nostack)
        );
        if old != expected {
            return Err(KernelError::CapInvalidToken);
        }
    }
    if let Some(stored) = &mut table.slots[slot] {
        maybe_tag_slot(stored as *mut Capability, new_generation);
    }
    table.slots[slot] = None;
    drop(table);
    unsafe {
        let sgir = (0xFFFF_0000_0800_0000u64 + 0xF00) as *mut u32;
        sgir.write_volatile(1 << 24);
    }
    cache::cap_cache_flush_local();
    Ok(())
}

pub fn check_right(token: CapToken, right: Rights) -> Result<(), KernelError> {
    bti_c!();
    let mut table = CAP_TABLE.lock();
    let (_slot, authed) = locked_validate(&mut table, token)?;
    if !Rights(authed.rights()).contains(right) {
        unsafe {
            core::arch::asm!("csdb", options(nomem, nostack, preserves_flags));
        }
        return Err(KernelError::CapInsufficientRights);
    }
    Ok(())
}

pub fn check_right_as(token: CapToken, right: Rights, caller_pid: u16) -> Result<(), KernelError> {
    bti_c!();
    let mut table = CAP_TABLE.lock();
    let (_slot, authed) = locked_validate(&mut table, token)?;
    if authed.owner_pid() != caller_pid {
        unsafe {
            core::arch::asm!("csdb", options(nomem, nostack, preserves_flags));
        }
        return Err(KernelError::CapInvalidToken);
    }
    if !Rights(authed.rights()).contains(right) {
        unsafe {
            core::arch::asm!("csdb", options(nomem, nostack, preserves_flags));
        }
        return Err(KernelError::CapInsufficientRights);
    }
    Ok(())
}

pub fn get_resource_id(token: CapToken) -> Result<u32, KernelError> {
    bti_c!();
    let mut table = CAP_TABLE.lock();
    let (slot, _authed) = locked_validate(&mut table, token)?;
    Ok(table.slots[slot].unwrap().resource_id)
}

pub fn get_rights(token: CapToken) -> Result<u16, KernelError> {
    bti_c!();
    validate(token)?;
    Ok(token.rights())
}

pub fn find_by_lo(lo: u64) -> Option<CapToken> {
    bti_c!();
    let table = CAP_TABLE.lock();
    table
        .slots
        .iter()
        .flatten()
        .find(|cap| cap.valid && cap.token.lo() == lo)
        .map(|cap| cap.token)
}

pub fn find_channel_cap(owner_pid: u16, resource_id: u32) -> Option<CapToken> {
    bti_c!();
    let table = CAP_TABLE.lock();
    table
        .slots
        .iter()
        .flatten()
        .find(|capability| {
            capability.valid
                && capability.token.owner_pid() == owner_pid
                && capability.token.resource_type() == ResourceType::Channel as u16
                && capability.resource_id == resource_id
        })
        .map(|capability| capability.token)
}

pub fn get_intent_id(token: CapToken) -> u16 {
    bti_c!();
    token.intent_id()
}

pub fn get_intent_class(token: CapToken) -> Result<IntentClass, KernelError> {
    bti_c!();
    let mut table = CAP_TABLE.lock();
    let (slot, _authed) = locked_validate(&mut table, token)?;
    table.slots[slot]
        .map(|cap| cap.intent_class)
        .ok_or(KernelError::CapInvalidToken)
}

pub fn delegate(
    token: CapToken,
    new_owner_pid: u16,
    reduced_rights: Rights,
) -> Result<CapToken, KernelError> {
    bti_c!();
    let mut table = CAP_TABLE.lock();
    let (slot, authed) = locked_validate(&mut table, token)?;
    let Some(cap) = table.slots[slot].as_ref() else {
        return Err(KernelError::CapInvalidToken);
    };
    if !Rights(authed.rights()).contains(Rights::GRANT) {
        return Err(KernelError::CapInsufficientRights);
    }
    if cap.delegation_depth >= cap.max_depth {
        return Err(KernelError::CapDelegationDepthExceeded);
    }
    let parent_slot = token.slot_index();
    let delegation_depth = cap.delegation_depth;
    let max_depth = cap.max_depth;
    let intent_class = cap.intent_class;
    drop(table);
    let child_rights = Rights(authed.rights() & reduced_rights.0);
    let child = create(
        new_owner_pid,
        match authed.resource_type() {
            0x0001 => ResourceType::Memory,
            0x0002 => ResourceType::Channel,
            0x0003 => ResourceType::Process,
            0x0004 => ResourceType::Interrupt,
            0x0005 => ResourceType::Intent,
            0x0006 => ResourceType::Telemetry,
            0x0007 => ResourceType::Network,
            0x000A => ResourceType::File,
            _ => ResourceType::Crypto,
        },
        get_resource_id(token)?,
        child_rights,
        authed.intent_id(),
        intent_class,
    )?;
    let mut table = CAP_TABLE.lock();
    let child_slot = child.slot_index() as usize;
    if let Some(stored) = table.slots[child_slot].as_mut() {
        stored.parent_slot = Some(parent_slot);
        stored.delegation_depth = delegation_depth.saturating_add(1);
        stored.max_depth = max_depth;
    }
    Ok(child)
}
