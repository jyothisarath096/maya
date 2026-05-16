#![allow(dead_code)]

pub mod elf;
pub mod inject;
pub mod mlm;
pub mod syscall;

use core::sync::atomic::{AtomicU16, Ordering};

use crate::{
    cap::{self, CapToken, IntentClass, ResourceType, Rights},
    cap::table::RawSpinLock,
    sched::process::{Process, ProcessClass},
    KernelError,
};

const MAX_PROCESSES: usize = 32;
const MAX_CAPS_PER_PROC: usize = 16;
const MAX_SEGMENTS_PER_PROC: usize = 8;
const MAX_INTENTS_PER_PROC: usize = 16;
const MAX_ALLOCATIONS_PER_PROC: usize = 16;
const MAX_PAGES_PER_ALLOCATION: usize = 32;
const MAX_NAME_LEN: usize = 32;
const MAX_CORES: usize = 8;
const USER_ALLOC_BASE: u64 = 0x4300_0000;
const USER_ALLOC_STRIDE: u64 = 0x0100_0000;
const PAGE_SIZE: u64 = 4096;

const ZERO_TOKEN: CapToken = CapToken(0);

#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct ContextFrame {
    pub x: [u64; 31],
    pub elr: u64,
    pub spsr: u64,
    pub sp_el0: u64,
    pub _pad0: u64,
    pub _pad1: u64,
    pub q: [u128; 32],
    pub fpcr: u64,
    pub fpsr: u64,
    pub _tail_pad: u64,
}

impl ContextFrame {
    pub const fn zeroed() -> Self {
        Self {
            x: [0; 31],
            elr: 0,
            spsr: 0,
            sp_el0: 0,
            _pad0: 0,
            _pad1: 0,
            q: [0; 32],
            fpcr: 0,
            fpsr: 0,
            _tail_pad: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct MemoryAllocation {
    valid: bool,
    resource_id: u32,
    vaddr: u64,
    size: u64,
    page_count: u16,
    intent_class: IntentClass,
    frames: [u64; MAX_PAGES_PER_ALLOCATION],
}

const EMPTY_ALLOCATION: MemoryAllocation = MemoryAllocation {
    valid: false,
    resource_id: 0,
    vaddr: 0,
    size: 0,
    page_count: 0,
    intent_class: IntentClass::Unknown,
    frames: [0; MAX_PAGES_PER_ALLOCATION],
};

#[repr(C, align(8))]
#[derive(Clone, Copy)]
struct ProcessEntry {
    valid: bool,
    pid: u16,
    name: [u8; MAX_NAME_LEN],
    name_len: u8,
    state: u8,
    entry: u64,
    stack_top: u64,
    ttbr0: u64,
    asid: u16,
    cap_count: u8,
    caps: [CapToken; MAX_CAPS_PER_PROC],
    intent_count: u8,
    intent_ids: [u16; MAX_INTENTS_PER_PROC],
    intent_caps: [CapToken; MAX_INTENTS_PER_PROC],
    intent_vaddrs: [u64; MAX_INTENTS_PER_PROC],
    intent_class: IntentClass,
    alloc_cursor: u64,
    next_mem_resource: u32,
    allocations: [MemoryAllocation; MAX_ALLOCATIONS_PER_PROC],
    inject_state: Option<inject::InjectionState>,
    inject_result: Option<inject::InjectionResult>,
    inject_return_vaddr: u64,
    kernel_stack: [u8; 8192],
    kernel_sp: u64,
    ctx_frame: ContextFrame,
    ctx_valid: bool,
}

struct ProcessTable {
    entries: [ProcessEntry; MAX_PROCESSES],
    count: usize,
    next_pid: u16,
    next_asid: u16,
}

const EMPTY_PROCESS: ProcessEntry = ProcessEntry {
    valid: false,
    pid: 0,
    name: [0; MAX_NAME_LEN],
    name_len: 0,
    state: 0,
    entry: 0,
    stack_top: 0,
    ttbr0: 0,
    asid: 1,
    cap_count: 0,
    caps: [ZERO_TOKEN; MAX_CAPS_PER_PROC],
    intent_count: 0,
    intent_ids: [0; MAX_INTENTS_PER_PROC],
    intent_caps: [ZERO_TOKEN; MAX_INTENTS_PER_PROC],
    intent_vaddrs: [0; MAX_INTENTS_PER_PROC],
    intent_class: IntentClass::Unknown,
    alloc_cursor: 0,
    next_mem_resource: 1,
    allocations: [EMPTY_ALLOCATION; MAX_ALLOCATIONS_PER_PROC],
    inject_state: None,
    inject_result: None,
    inject_return_vaddr: 0,
    kernel_stack: [0; 8192],
    kernel_sp: 0,
    ctx_frame: ContextFrame::zeroed(),
    ctx_valid: false,
};

const EMPTY_TABLE: ProcessTable = ProcessTable {
    entries: [EMPTY_PROCESS; MAX_PROCESSES],
    count: 0,
    next_pid: 4,
    next_asid: 1,
};

static PROC_TABLE: RawSpinLock<ProcessTable> = RawSpinLock::new(EMPTY_TABLE);
static CURRENT_PID: AtomicU16 = AtomicU16::new(0);
static CURRENT_PROC: [AtomicU16; MAX_CORES] = [const { AtomicU16::new(0) }; MAX_CORES];

pub fn init() {
    let mut table = PROC_TABLE.lock();
    *table = EMPTY_TABLE;
    CURRENT_PID.store(0, Ordering::Release);
    for slot in &CURRENT_PROC {
        slot.store(0, Ordering::Release);
    }
}

pub fn snapshot_process_display_with_core(
    mut f: impl FnMut(u16, u8, &[u8], IntentClass, crate::sched::process::ProcessStats),
) {
    let table = PROC_TABLE.lock();
    for entry in table.entries.iter() {
        if !entry.valid {
            continue;
        }
        let len = entry.name_len as usize;
        let name = &entry.name[..len];
        let (core_id, stats) = crate::sched::queue::get_process_with_core(entry.pid)
            .map(|(core_id, process)| (core_id, process.stats))
            .unwrap_or((
                0,
                crate::sched::process::ProcessStats {
                    cpu_ticks_used: 0,
                    ticks_waiting: 0,
                    io_wait_ticks: 0,
                    ipc_sends: 0,
                    ipc_recvs: 0,
                    alarms_sent: 0,
                    alarms_acked: 0,
                    file_reads: 0,
                    file_writes: 0,
                    file_bytes_written: 0,
                    page_faults: 0,
                    last_scheduled: 0,
                    last_input_tick: 0,
                    burst_ticks: 0,
                    total_ticks_alive: 0,
                    last_intent_id: 0,
                    intent_fire_count: 0,
                    intent_fire_ns: 0,
                },
            ));
        f(entry.pid, core_id, name, entry.intent_class, stats);
    }
}

pub fn snapshot_process_display(
    mut f: impl FnMut(u16, &[u8], IntentClass, crate::sched::process::ProcessStats),
) {
    snapshot_process_display_with_core(|pid, _core_id, name, intent_class, stats| {
        f(pid, name, intent_class, stats);
    });
}

pub fn snapshot_for_telemetry<F>(mut f: F) -> bool
where
    F: FnMut(crate::telemetry::ProcSnap),
{
    let Some(table) = PROC_TABLE.try_lock() else {
        return false;
    };
    for entry in table.entries.iter() {
        if !entry.valid {
            continue;
        }
        let nlen = (entry.name_len as usize).min(12);
        let mut snap = crate::telemetry::ProcSnap::empty();
        snap.valid = true;
        snap.pid = entry.pid;
        snap.intent = entry.intent_class as u8;
        snap.name_len = nlen as u8;
        snap.name[..nlen].copy_from_slice(&entry.name[..nlen]);
        let sched_data = crate::sched::queue::get_process_with_core_try(entry.pid);
        let (core_id, cpu, ipc_s, ipc_r, fires, fw, pkt) = if let Some((core_id, process)) = sched_data {
            snap.alm = process.stats.alarms_sent;
            snap.ack = process.stats.alarms_acked;
            (
                core_id,
                process.stats.cpu_ticks_used,
                process.stats.ipc_sends,
                process.stats.ipc_recvs,
                process.stats.intent_fire_count,
                process.stats.file_writes,
                if entry.pid == 8 {
                    process.stats.intent_fire_count
                } else {
                    0
                },
            )
        } else {
            (0u8, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64)
        };
        snap.core_id = core_id;
        snap.cpu = cpu;
        snap.ipc_s = ipc_s;
        snap.ipc_r = ipc_r;
        snap.fires = fires;
        snap.fw = fw;
        snap.pkt = pkt;
        f(snap);
    }
    true
}

pub fn set_current_pid(pid: u16) {
    CURRENT_PID.store(pid, Ordering::Release);
}

pub fn get_current_pid() -> u16 {
    CURRENT_PID.load(Ordering::Acquire)
}

pub fn current_process_for_core(core_id: usize) -> u16 {
    CURRENT_PROC
        .get(core_id)
        .map(|pid| pid.load(Ordering::Acquire))
        .unwrap_or(0)
}

pub fn set_current_process_for_core(core_id: usize, pid: u16) {
    if let Some(slot) = CURRENT_PROC.get(core_id) {
        slot.store(pid, Ordering::Release);
    }
}

pub fn set_current_proc(core_id: usize, pid: u16) {
    set_current_process_for_core(core_id, pid);
}

pub fn allocate_intent_id(pid: u16) -> u16 {
    let table = PROC_TABLE.lock();
    if let Some(entry) = table.entries.iter().find(|entry| entry.valid && entry.pid == pid) {
        0x100u16.saturating_add(pid.saturating_mul(MAX_INTENTS_PER_PROC as u16))
            .saturating_add(entry.intent_count as u16)
    } else {
        0x100u16.saturating_add(pid)
    }
}

pub fn register_process_intent(pid: u16, intent_id: u16, token: CapToken) -> Result<(), KernelError> {
    let mut table = PROC_TABLE.lock();
    let Some(entry) = table.entries.iter_mut().find(|entry| entry.valid && entry.pid == pid) else {
        return Err(KernelError::ProcNotFound);
    };
    let index = entry.intent_count as usize;
    if index >= MAX_INTENTS_PER_PROC {
        return Err(KernelError::ProcTableFull);
    }
    entry.intent_ids[index] = intent_id;
    entry.intent_caps[index] = token;
    entry.intent_vaddrs[index] = 0;
    entry.intent_count = entry.intent_count.saturating_add(1);
    Ok(())
}

pub fn register_process_intent_vaddr(pid: u16, intent_id: u16, vaddr: u64) -> Result<(), KernelError> {
    let mut table = PROC_TABLE.lock();
    let Some(entry) = table.entries.iter_mut().find(|entry| entry.valid && entry.pid == pid) else {
        return Err(KernelError::ProcNotFound);
    };
    for index in 0..entry.intent_count as usize {
        if entry.intent_ids[index] == intent_id {
            entry.intent_vaddrs[index] = vaddr;
            return Ok(());
        }
    }
    Err(KernelError::ProcNotFound)
}

pub fn get_intent_vaddr(pid: u16, intent_id: u16) -> Option<u64> {
    let table = PROC_TABLE.lock();
    let entry = table.entries.iter().find(|entry| entry.valid && entry.pid == pid)?;
    for index in 0..entry.intent_count as usize {
        if entry.intent_ids[index] == intent_id {
            return Some(entry.intent_vaddrs[index]);
        }
    }
    None
}

pub fn get_process_intent_cap(pid: u16, intent_id: u16) -> Option<CapToken> {
    let table = PROC_TABLE.lock();
    let entry = table.entries.iter().find(|entry| entry.valid && entry.pid == pid)?;
    for index in 0..entry.intent_count as usize {
        if entry.intent_ids[index] == intent_id {
            return Some(entry.intent_caps[index]);
        }
    }
    None
}

pub fn update_process_intent_class(pid: u16, class: IntentClass) {
    let mut table = PROC_TABLE.lock();
    if let Some(entry) = table.entries.iter_mut().find(|entry| entry.valid && entry.pid == pid) {
        entry.intent_class = class;
    }
    let _ = crate::sched::queue::set_process_intent_class(pid, class);
}

pub fn get_process_intent_class(pid: u16) -> IntentClass {
    let table = PROC_TABLE.lock();
    table
        .entries
        .iter()
        .find(|entry| entry.valid && entry.pid == pid)
        .map(|entry| entry.intent_class)
        .unwrap_or(IntentClass::Unknown)
}

pub fn set_inject_return_vaddr(pid: u16, vaddr: u64) {
    let mut table = PROC_TABLE.lock();
    if let Some(entry) = table.entries.iter_mut().find(|entry| entry.valid && entry.pid == pid) {
        entry.inject_return_vaddr = vaddr;
    }
}

pub fn get_inject_return_vaddr(pid: u16) -> Option<u64> {
    let table = PROC_TABLE.lock();
    table
        .entries
        .iter()
        .find(|entry| entry.valid && entry.pid == pid)
        .map(|entry| entry.inject_return_vaddr)
        .filter(|vaddr| *vaddr != 0)
}

pub fn set_injection_state(pid: u16, state: inject::InjectionState) {
    let mut table = PROC_TABLE.lock();
    if let Some(entry) = table.entries.iter_mut().find(|entry| entry.valid && entry.pid == pid) {
        entry.inject_state = Some(state);
    }
}

pub fn get_injection_state(pid: u16) -> Option<inject::InjectionState> {
    let table = PROC_TABLE.lock();
    table
        .entries
        .iter()
        .find(|entry| entry.valid && entry.pid == pid)
        .and_then(|entry| entry.inject_state)
}

pub fn clear_injection_state(pid: u16) {
    let mut table = PROC_TABLE.lock();
    if let Some(entry) = table.entries.iter_mut().find(|entry| entry.valid && entry.pid == pid) {
        entry.inject_state = None;
    }
}

pub fn store_injection_result(pid: u16, result: inject::InjectionResult) {
    let mut table = PROC_TABLE.lock();
    if let Some(entry) = table.entries.iter_mut().find(|entry| entry.valid && entry.pid == pid) {
        entry.inject_result = Some(result);
    }
}

pub fn set_injection_result(pid: u16, intent_id: u16, lo: u64, hi: u64, ns: u64) {
    store_injection_result(
        pid,
        inject::InjectionResult {
            intent_id,
            return_lo: lo,
            return_hi: hi,
            latency_ns: ns,
            status: inject::InjectionStatus::Complete,
        },
    );
}

pub fn create_process_entry(name: &[u8], loaded: &elf::LoadedElf, intent_class: IntentClass) -> Result<u16, KernelError> {
    let mut table = PROC_TABLE.lock();
    let Some(index) = table.entries.iter().position(|entry| !entry.valid) else {
        return Err(KernelError::ProcTableFull);
    };
    let pid = table.next_pid;
    table.next_pid = table.next_pid.saturating_add(1);
    let asid = table.next_asid;
    table.next_asid = table.next_asid.saturating_add(1);

    let entry = &mut table.entries[index];
    entry.valid = true;
    entry.pid = pid;
    entry.name_len = name.len().min(MAX_NAME_LEN) as u8;
    entry.name[..entry.name_len as usize].copy_from_slice(&name[..entry.name_len as usize]);
    entry.state = 0;
    entry.entry = loaded.entry;
    entry.stack_top = loaded.stack_top;
    entry.ttbr0 = loaded.ttbr0;
    entry.asid = asid;
    entry.intent_class = intent_class;
    entry.alloc_cursor = alloc_base_for_slot(elf::slot_for_entry(loaded.entry));
    entry.next_mem_resource = 1;
    entry.allocations = [EMPTY_ALLOCATION; MAX_ALLOCATIONS_PER_PROC];
    entry.kernel_sp = 0;
    entry.ctx_frame = ContextFrame::zeroed();
    entry.ctx_valid = false;
    table.count = table.count.saturating_add(1);
    Ok(pid)
}

pub fn get_process_name(pid: u16) -> Option<[u8; MAX_NAME_LEN]> {
    let table = PROC_TABLE.lock();
    let entry = table.entries.iter().find(|entry| entry.valid && entry.pid == pid)?;
    let mut out = [0u8; MAX_NAME_LEN];
    let len = entry.name_len as usize;
    out[..len].copy_from_slice(&entry.name[..len]);
    Some(out)
}

pub fn create_proc_entries(pid: u16) {
    let now = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct());
    let mut path = [0u8; 32];

    let plen = build_proc_path(&mut path, pid, b"stats");
    if let Some(fid) = crate::fs::store::alloc_virtual(
        pid,
        crate::fs::store::VFILE_PROC_STATS | ((pid as u32) << 16),
        now,
    ) {
        crate::fs::namespace::insert_path(&path[..plen], fid, false).ok();
    }

    let plen = build_proc_path(&mut path, pid, b"intent");
    if let Some(fid) = crate::fs::store::alloc_virtual(
        pid,
        crate::fs::store::VFILE_PROC_INTENT | ((pid as u32) << 16),
        now,
    ) {
        crate::fs::namespace::insert_path(&path[..plen], fid, false).ok();
    }

    let plen = build_proc_path(&mut path, pid, b"name");
    if let Some(fid) = crate::fs::store::alloc_virtual(
        pid,
        crate::fs::store::VFILE_PROC_NAME | ((pid as u32) << 16),
        now,
    ) {
        crate::fs::namespace::insert_path(&path[..plen], fid, false).ok();
    }
}

fn build_proc_path(buf: &mut [u8], pid: u16, suffix: &[u8]) -> usize {
    buf[..6].copy_from_slice(b"/proc/");
    let mut pos = 6;
    let mut tmp = [0u8; 5];
    let mut tlen = 0usize;
    let mut p = pid as u32;
    if p == 0 {
        tmp[0] = b'0';
        tlen = 1;
    } else {
        while p > 0 {
            tmp[tlen] = b'0' + (p % 10) as u8;
            p /= 10;
            tlen += 1;
        }
        tmp[..tlen].reverse();
    }
    buf[pos..pos + tlen].copy_from_slice(&tmp[..tlen]);
    pos += tlen;
    buf[pos] = b'/';
    pos += 1;
    let slen = suffix.len();
    buf[pos..pos + slen].copy_from_slice(suffix);
    pos + slen
}

pub fn launch_agentic_aarch64(
    name: &str,
    elf_data: &[u8],
    shim_data: &[u8],
    manifest_data: &[u8],
    auto_schedule: bool,
) -> Result<u16, KernelError> {
    let mlmb = mlm::parse_mlmb(manifest_data).ok_or(KernelError::InvalidElf)?;
    let primary_entry = mlmb.entries.first().copied();
    let primary_class = primary_entry.map(|entry| entry.intent_class).unwrap_or(IntentClass::Unknown);

    let loaded = elf::load(elf_data, next_asid())?;
    let slot = elf::slot_for_entry(loaded.entry);
    let shim_load_addr = if mlmb.shim_load_addr != 0 {
        mlmb.shim_load_addr
    } else {
        0x47F0_0000 + (slot as u64 * 0x0001_0000)
    };
    let scratch_addr = if mlmb.scratch_addr != 0 {
        mlmb.scratch_addr
    } else {
        loaded.scratch_addr
    };
    let guard_addr = if mlmb.guard_addr != 0 {
        mlmb.guard_addr
    } else {
        0x47FF_F000 + (slot as u64 * 0x1000)
    };
    elf::map_agentic_runtime(loaded.ttbr0, shim_data, shim_load_addr, scratch_addr, guard_addr)
        .map_err(|_| KernelError::ElfLoadFailed)?;

    let pid = create_process_entry(name.as_bytes(), &loaded, primary_class)?;
    create_proc_entries(pid);
    set_current_pid(pid);
    set_inject_return_vaddr(pid, mlmb.inject_return_vaddr);
    create_console_cap(pid)?;

    for entry in mlmb.entries.iter().copied() {
        let token = cap::create(
            pid,
            ResourceType::Intent,
            entry.intent_id as u32,
            Rights(entry.cap_rights),
            entry.intent_id,
            entry.intent_class,
        )?;
        register_process_intent(pid, entry.intent_id, token)?;
        register_process_intent_vaddr(pid, entry.intent_id, entry.entry_vaddr)?;
    }

    if auto_schedule {
        let mut process = Process::new(pid, process_class_for_intent(primary_class), 1.0);
        process.intent_class = primary_class;
        process.priority_hint = 1.0;
        crate::sched::queue::add_process(process);

        if let Some(primary) = primary_entry {
            crate::sched::queue::update_process_intent(
                pid,
                primary.intent_id,
                crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct()),
            );
            update_process_intent_class(pid, primary.intent_class);
        }
    }

    Ok(pid)
}

pub fn add_process_to_queue(pid: u16, class: ProcessClass, priority: f32) {
    let mut process = Process::new(pid, class, priority);
    process.intent_class = match class {
        ProcessClass::Realtime => IntentClass::RealTime,
        ProcessClass::Interactive => IntentClass::Compute,
        ProcessClass::Batch => IntentClass::Background,
        ProcessClass::Idle => IntentClass::Unknown,
    };
    process.priority_hint = priority;
    crate::sched::queue::add_process(process);
}

pub fn add_process_to_core(core_id: u8, pid: u16, class: ProcessClass, priority: f32) {
    let mut process = Process::new(pid, class, priority);
    process.intent_class = match class {
        ProcessClass::Realtime => IntentClass::RealTime,
        ProcessClass::Interactive => IntentClass::Compute,
        ProcessClass::Batch => IntentClass::Background,
        ProcessClass::Idle => IntentClass::Unknown,
    };
    process.priority_hint = priority;
    crate::sched::queue::add_process_to_core(core_id, process);
}

pub fn save_process_frame(pid: u16, frame: *mut ContextFrame) {
    let mut table = PROC_TABLE.lock();
    if let Some(entry) = table.entries.iter_mut().find(|entry| entry.valid && entry.pid == pid) {
        unsafe {
            entry.ctx_frame = *frame;
        }
        entry.ctx_valid = true;
    }
}

pub fn save_syscall_frame(pid: u16, frame: &crate::arch::exceptions::SyscallFrame) {
    let mut table = PROC_TABLE.lock();
    if let Some(entry) = table.entries.iter_mut().find(|entry| entry.valid && entry.pid == pid) {
        entry.ctx_frame.x = frame.x;
        entry.ctx_frame.elr = frame.elr;
        entry.ctx_frame.spsr = frame.spsr;
        entry.ctx_frame.sp_el0 = frame.sp_el0;
        entry.ctx_valid = true;
    }
}

pub fn load_syscall_frame(pid: u16, frame: &mut crate::arch::exceptions::SyscallFrame) -> Option<u64> {
    let mut table = PROC_TABLE.lock();
    let entry = table.entries.iter_mut().find(|entry| entry.valid && entry.pid == pid)?;
    if !entry.ctx_valid {
        entry.ctx_frame = ContextFrame::zeroed();
        entry.ctx_frame.elr = entry.entry;
        entry.ctx_frame.spsr = 0;
        entry.ctx_frame.sp_el0 = entry.stack_top;
        entry.ctx_valid = true;
    }
    frame.x = entry.ctx_frame.x;
    frame.elr = entry.ctx_frame.elr;
    frame.spsr = entry.ctx_frame.spsr;
    frame.sp_el0 = entry.ctx_frame.sp_el0;
    Some(entry.ctx_frame.sp_el0)
}

pub fn get_process_frame(pid: u16) -> *mut ContextFrame {
    let mut table = PROC_TABLE.lock();
    if let Some(entry) = table.entries.iter_mut().find(|entry| entry.valid && entry.pid == pid) {
        if !entry.ctx_valid {
            let was_running = entry.ctx_frame.elr != 0 && entry.ctx_frame.elr != entry.entry;
            if was_running {
                unsafe {
                    let uart = 0xFFFF_0000_0900_0000u64 as *mut u32;
                    uart.write_volatile(b'X' as u32);
                    let p = entry.pid as u8;
                    let hi = p >> 4;
                    let lo = p & 0xF;
                    uart.write_volatile((if hi < 10 { b'0' + hi } else { b'A' + hi - 10 }) as u32);
                    uart.write_volatile((if lo < 10 { b'0' + lo } else { b'A' + lo - 10 }) as u32);
                }
            }
            entry.ctx_frame = ContextFrame::zeroed();
            entry.ctx_frame.elr = entry.entry;
            entry.ctx_frame.spsr = 0;
            entry.ctx_frame.sp_el0 = entry.stack_top;
            entry.ctx_valid = true;
        }
        return &mut entry.ctx_frame as *mut ContextFrame;
    }
    core::ptr::null_mut()
}

pub fn get_process_ttbr0(pid: u16) -> Option<(u64, u16)> {
    let table = PROC_TABLE.lock();
    table
        .entries
        .iter()
        .find(|entry| entry.valid && entry.pid == pid)
        .map(|entry| (entry.ttbr0, entry.asid))
}

pub fn get_process_launch_params(pid: u16) -> Option<(u64, u64, u64, u16)> {
    let table = PROC_TABLE.lock();
    let entry = table.entries.iter().find(|entry| entry.valid && entry.pid == pid)?;
    Some((entry.entry, entry.stack_top, entry.ttbr0, entry.asid))
}

pub fn allocate_process_memory(
    pid: u16,
    size: usize,
    alignment: usize,
    intent_class: IntentClass,
) -> Result<(u64, CapToken), KernelError> {
    let alignment = alignment.max(8);
    if !alignment.is_power_of_two() || size == 0 {
        return Err(KernelError::VmmMapFailed);
    }
    let page_count = (size as u64).div_ceil(PAGE_SIZE) as usize;
    if page_count == 0 || page_count > MAX_PAGES_PER_ALLOCATION {
        return Err(KernelError::VmmMapFailed);
    }

    let (ttbr0, vaddr, resource_id, alloc_index) = {
        let mut table = PROC_TABLE.lock();
        let entry = table
            .entries
            .iter_mut()
            .find(|entry| entry.valid && entry.pid == pid)
            .ok_or(KernelError::ProcNotFound)?;
        let alloc_index = entry
            .allocations
            .iter()
            .position(|alloc| !alloc.valid)
            .ok_or(KernelError::ProcTableFull)?;
        let aligned_vaddr = (entry.alloc_cursor + alignment as u64 - 1) & !((alignment as u64) - 1);
        entry.alloc_cursor = aligned_vaddr + page_count as u64 * PAGE_SIZE;
        let resource_id = entry.next_mem_resource;
        entry.next_mem_resource = entry.next_mem_resource.saturating_add(1);
        (entry.ttbr0, aligned_vaddr, resource_id, alloc_index)
    };

    let mut frames = [0u64; MAX_PAGES_PER_ALLOCATION];
    for frame in frames.iter_mut().take(page_count) {
        let phys = crate::memory::pmm::alloc_frame().ok_or(KernelError::ElfLoadFailed)?;
        unsafe {
            core::ptr::write_bytes(
                crate::memory::pmm::phys_to_virt(phys) as *mut u8,
                0,
                PAGE_SIZE as usize,
            );
        }
        *frame = phys;
    }

    if let Err(err) = crate::memory::vmm::map_user_frames(ttbr0, vaddr, &frames[..page_count], true, false) {
        for phys in frames.iter().copied().take(page_count) {
            if phys != 0 {
                crate::memory::pmm::free_frame(phys);
            }
        }
        return Err(err);
    }

    let token = match cap::create(
        pid,
        ResourceType::Memory,
        resource_id,
        Rights(Rights::READ.0 | Rights::WRITE.0),
        0x110,
        intent_class,
    ) {
        Ok(token) => token,
        Err(err) => {
            crate::memory::vmm::unmap_user_range(ttbr0, vaddr, page_count * PAGE_SIZE as usize);
            for phys in frames.iter().copied().take(page_count) {
                if phys != 0 {
                    crate::memory::pmm::free_frame(phys);
                }
            }
            return Err(err);
        }
    };

    let mut table = PROC_TABLE.lock();
    let entry = table
        .entries
        .iter_mut()
        .find(|entry| entry.valid && entry.pid == pid)
        .ok_or(KernelError::ProcNotFound)?;
    entry.allocations[alloc_index] = MemoryAllocation {
        valid: true,
        resource_id,
        vaddr,
        size: size as u64,
        page_count: page_count as u16,
        intent_class,
        frames,
    };

    Ok((vaddr, token))
}

pub fn create_console_cap(pid: u16) -> Result<CapToken, KernelError> {
    let token = cap::create(
        pid,
        crate::cap::ResourceType::Console,
        1,
        Rights(Rights::WRITE.0 | Rights::OBSERVE.0),
        0x101,
        crate::cap::IntentClass::IO,
    )?;

    let mut table = PROC_TABLE.lock();
    let entry = table
        .entries
        .iter_mut()
        .find(|entry| entry.valid && entry.pid == pid)
        .ok_or(KernelError::ProcNotFound)?;
    entry.caps[0] = token;
    entry.cap_count = entry.cap_count.max(1);
    Ok(token)
}

pub fn set_process_x0(pid: u16, value: u64) {
    let mut table = PROC_TABLE.lock();
    if let Some(entry) = table.entries.iter_mut().find(|e| e.valid && e.pid == pid) {
        entry.ctx_frame.x[0] = value;
        entry.ctx_valid = true;
        if entry.ctx_frame.elr == 0 {
            entry.ctx_frame.elr = entry.entry;
        }
        if entry.ctx_frame.sp_el0 == 0 {
            entry.ctx_frame.sp_el0 = entry.stack_top;
        }
    }
}

pub fn get_process_console_cap(pid: u16) -> Option<CapToken> {
    let table = PROC_TABLE.lock();
    let entry = table
        .entries
        .iter()
        .find(|entry| entry.valid && entry.pid == pid)?;
    if entry.cap_count == 0 {
        return None;
    }
    let token = entry.caps[0];
    if token.0 == 0 {
        return None;
    }
    Some(token)
}

pub fn free_process_memory(pid: u16, token: CapToken) -> Result<(), KernelError> {
    cap::check_right_as(token, Rights::WRITE, pid)?;
    let resource_id = cap::get_resource_id(token)?;
    let (ttbr0, vaddr, size, frames) = {
        let mut table = PROC_TABLE.lock();
        let entry = table
            .entries
            .iter_mut()
            .find(|entry| entry.valid && entry.pid == pid)
            .ok_or(KernelError::ProcNotFound)?;
        let alloc = entry
            .allocations
            .iter_mut()
            .find(|alloc| alloc.valid && alloc.resource_id == resource_id)
            .ok_or(KernelError::CapInvalidToken)?;
        let frames = alloc.frames;
        let page_count = alloc.page_count as usize;
        let size = alloc.size as usize;
        let vaddr = alloc.vaddr;
        alloc.valid = false;
        alloc.resource_id = 0;
        alloc.vaddr = 0;
        alloc.size = 0;
        alloc.page_count = 0;
        alloc.intent_class = IntentClass::Unknown;
        alloc.frames = [0; MAX_PAGES_PER_ALLOCATION];
        (entry.ttbr0, vaddr, size.max(page_count * PAGE_SIZE as usize), frames)
    };

    crate::memory::vmm::unmap_user_range(ttbr0, vaddr, size);
    cap::revoke(token)?;
    for phys in frames {
        if phys != 0 {
            crate::memory::pmm::free_frame(phys);
        }
    }
    Ok(())
}

pub fn free_process_memory_by_lo(pid: u16, cap_lo: u64) -> Result<(), KernelError> {
    let token = cap::find_by_lo(cap_lo).ok_or(KernelError::CapInvalidToken)?;
    free_process_memory(pid, token)
}

fn next_asid() -> u16 {
    let mut table = PROC_TABLE.lock();
    let asid = table.next_asid;
    table.next_asid = table.next_asid.wrapping_add(1) & 0xFF;
    asid
}

fn process_class_for_intent(class: IntentClass) -> ProcessClass {
    match class {
        IntentClass::RealTime => ProcessClass::Realtime,
        IntentClass::Background => ProcessClass::Batch,
        IntentClass::Unknown => ProcessClass::Batch,
        IntentClass::Compute | IntentClass::IO | IntentClass::System => ProcessClass::Interactive,
    }
}

fn alloc_base_for_slot(slot: usize) -> u64 {
    USER_ALLOC_BASE + slot as u64 * USER_ALLOC_STRIDE
}

pub unsafe fn jump_to_el0(entry: u64, stack: u64, ttbr0: u64, asid: u16) -> ! {
    let core_id = crate::arch::cpu::current_core_id() as u64;
    let kstack = 0xFFFF_0000_4010_8000u64 - core_id * 0x10000;
    let ttbr0_val = ((asid as u64) << 48) | ttbr0;
    core::arch::asm!(
        "mov sp, {kstack}",
        "dsb ishst",
        "msr ttbr0_el1, {ttbr0}",
        "isb",
        "tlbi vmalle1",
        "dsb ish",
        "isb",
        "msr elr_el1, {entry}",
        "msr spsr_el1, xzr",
        "msr sp_el0, {stack}",
        "mov x0, xzr",
        "mov x1, xzr",
        "mov x2, xzr",
        "mov x3, xzr",
        "mov x4, xzr",
        "mov x5, xzr",
        "mov x6, xzr",
        "mov x7, xzr",
        "mov x8, xzr",
        "mov x9, xzr",
        "mov x10, xzr",
        "mov x11, xzr",
        "mov x12, xzr",
        "mov x13, xzr",
        "mov x14, xzr",
        "mov x15, xzr",
        "mov x16, xzr",
        "mov x17, xzr",
        "mov x18, xzr",
        "mov x19, xzr",
        "mov x20, xzr",
        "mov x21, xzr",
        "mov x22, xzr",
        "mov x23, xzr",
        "mov x24, xzr",
        "mov x25, xzr",
        "mov x26, xzr",
        "mov x27, xzr",
        "mov x28, xzr",
        "mov x29, xzr",
        "mov x30, xzr",
        "isb",
        "eret",
        kstack = in(reg) kstack,
        ttbr0 = in(reg) ttbr0_val,
        entry = in(reg) entry,
        stack = in(reg) stack,
        options(noreturn)
    );
}
