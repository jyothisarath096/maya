#![allow(dead_code)]

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use crate::{
    cap::{self, CapToken, ResourceType, Rights},
    fb_print,
    sched::{self, process::ProcessClass},
    serial_print,
    sync::TicketLock,
    KernelError,
};

pub mod elf;
pub mod memory;
pub mod syscall;

#[derive(Debug, Clone, PartialEq)]
pub enum ProcessState {
    Ready,
    Running,
    Blocked,
    Exited(i32),
}

#[derive(Clone)]
pub struct Process {
    pub pid: u16,
    pub name: String,
    pub state: ProcessState,
    pub entry: u64,
    pub stack_top: u64,
    pub cr3: u64,
    pub mapped_segments: Vec<(u64, usize)>,
    pub caps: Vec<CapToken>,
    pub stdout_cap: CapToken,
    pub stdin_cap: CapToken,
}

struct ProcessTable {
    processes: Vec<Process>,
    next_pid: u16,
    intent_registry: Vec<(u16, String)>,
}

static PROCTABLE: TicketLock<Option<ProcessTable>> = TicketLock::new(None);
static CURRENT_SEGMENTS: TicketLock<Vec<(u64, usize)>> = TicketLock::new(Vec::new());
static PROCESS_DONE: AtomicBool = AtomicBool::new(false);
static CURRENT_PID: AtomicU16 = AtomicU16::new(0);
static mut MAIN_LOOP_STACK: [u8; 65536] = [0u8; 65536];
static mut MAIN_LOOP_RSP: u64 = 0;
static mut MAIN_LOOP_FUNC: u64 = 0;

pub fn init() {
    *PROCTABLE.lock() = Some(ProcessTable {
        processes: Vec::new(),
        next_pid: 2,
        intent_registry: Vec::new(),
    });
    serial_print("Process table initialised\n");
    fb_print("Process table initialised\n");
}

pub fn spawn(
    name: &str,
    entry: u64,
    stack_top: u64,
    cr3: u64,
    mapped_segments: Vec<(u64, usize)>,
) -> u16 {
    let mut table_guard = PROCTABLE.lock();
    let table = table_guard.as_mut().expect("process table not initialised");
    let pid = table.next_pid;
    table.next_pid = table.next_pid.saturating_add(1);

    let stdout_cap =
        cap::create(pid, ResourceType::Channel, 0, Rights::WRITE).expect("stdout capability");
    let stdin_cap =
        cap::create(pid, ResourceType::Channel, 1, Rights::READ).expect("stdin capability");

    let mut caps = Vec::new();
    caps.push(stdout_cap);
    caps.push(stdin_cap);

    table.processes.push(Process {
        pid,
        name: name.to_string(),
        state: ProcessState::Ready,
        entry,
        stack_top,
        cr3,
        mapped_segments,
        caps,
        stdout_cap,
        stdin_cap,
    });

    drop(table_guard);
    let sched_proc = sched::process::Process::new(
        pid,
        ProcessClass::Interactive,
        0.5,
    );
    let target_core = sched::balance::assign_core_for_new_process();
    sched::queue::add_process(sched_proc.clone());
    sched::queue::add_process_to_core(target_core, sched_proc);
    pid
}

pub fn exit(pid: u16, code: i32) {
    let mut to_revoke = Vec::new();
    {
        let mut table_guard = PROCTABLE.lock();
        let Some(table) = table_guard.as_mut() else {
            return;
        };

        if let Some(process) = table.processes.iter_mut().find(|process| process.pid == pid) {
            process.state = ProcessState::Exited(code);
            to_revoke.extend(process.caps.iter().copied());
        }
    }

    for cap in to_revoke {
        cap::revoke(cap).ok();
    }
    sched::queue::remove_process(pid);
}

pub fn get(pid: u16) -> Option<Process> {
    PROCTABLE
        .lock()
        .as_ref()?
        .processes
        .iter()
        .find(|process| process.pid == pid)
        .cloned()
}

pub fn count() -> usize {
    PROCTABLE
        .lock()
        .as_ref()
        .map(|table| table.processes.len())
        .unwrap_or(0)
}

pub fn list() -> Vec<(u16, String, ProcessState)> {
    PROCTABLE
        .lock()
        .as_ref()
        .map(|table| {
            table
                .processes
                .iter()
                .map(|process| (process.pid, process.name.clone(), process.state.clone()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn register_intent(pid: u16, intent: &str) {
    let mut guard = PROCTABLE.lock();
    let Some(table) = guard.as_mut() else {
        return;
    };

    if let Some((_, value)) = table
        .intent_registry
        .iter_mut()
        .find(|(entry_pid, _)| *entry_pid == pid)
    {
        *value = intent.to_string();
        return;
    }

    table.intent_registry.push((pid, intent.to_string()));
}

pub fn get_intent(pid: u16) -> Option<String> {
    let guard = PROCTABLE.lock();
    let table = guard.as_ref()?;
    table
        .intent_registry
        .iter()
        .find(|(entry_pid, _)| *entry_pid == pid)
        .map(|(_, intent)| intent.clone())
}

pub fn clear_intent(pid: u16) {
    let mut guard = PROCTABLE.lock();
    if let Some(table) = guard.as_mut() {
        table.intent_registry.retain(|(entry_pid, _)| *entry_pid != pid);
    }
}

pub fn mark_process_done() {
    PROCESS_DONE.store(true, Ordering::Release);
}

pub fn process_is_done() -> bool {
    PROCESS_DONE.swap(false, Ordering::AcqRel)
}

pub fn set_current_pid(pid: u16) {
    CURRENT_PID.store(pid, Ordering::Release);
}

pub fn get_current_pid() -> u16 {
    CURRENT_PID.load(Ordering::Acquire)
}

pub fn set_current_segments(segs: Vec<(u64, usize)>) {
    *CURRENT_SEGMENTS.lock() = segs;
}

pub fn unmap_current_process() {
    let segs = CURRENT_SEGMENTS.lock().clone();
    memory::unmap_process_pages(&segs);
    CURRENT_SEGMENTS.lock().clear();
}

pub fn set_main_loop_return(func: u64) {
    unsafe {
        MAIN_LOOP_FUNC = func;
        MAIN_LOOP_RSP = core::ptr::addr_of!(MAIN_LOOP_STACK) as u64 + 65536;
    }
}

pub fn restore_to_main() -> ! {
    let (rsp, func) = unsafe { (MAIN_LOOP_RSP, MAIN_LOOP_FUNC) };
    unsafe {
        core::arch::asm!(
            "mov rsp, {rsp}",
            "jmp {func}",
            rsp = in(reg) rsp,
            func = in(reg) func,
            options(noreturn)
        );
    }
}

pub fn launch(name: &str, elf_data: &[u8]) -> Result<u16, KernelError> {
    if elf_data.is_empty() {
        return Err(KernelError::InvalidElf);
    }
    let loaded = crate::proc::elf::load(elf_data)?;
    set_current_segments(loaded.segments.clone());
    let pid = spawn(
        name,
        loaded.entry,
        loaded.stack_top,
        loaded.cr3,
        loaded.segments,
    );
    set_current_pid(pid);

    serial_print("Launching: ");
    serial_print(name);
    serial_print("\n");
    fb_print("Launching: ");
    fb_print(name);
    fb_print("\n");

    unsafe {
        jump_to_userspace(loaded.entry, loaded.stack_top, loaded.cr3);
    }
}

pub fn launch_agentic(
    name: &str,
    elf_data: &[u8],
    shim_data: &[u8],
) -> Result<u16, KernelError> {
    if elf_data.is_empty() {
        return Err(KernelError::InvalidElf);
    }

    let mut loaded = crate::proc::elf::load(elf_data)?;
    let shim_vaddr = 0x7F00_0000u64;
    crate::proc::memory::map_segment(loaded.cr3, shim_vaddr, shim_data, false, true)?;
    loaded.segments.push((shim_vaddr, shim_data.len()));
    let shim_flag_vaddr = 0x7FFF_FF00u64;
    crate::proc::memory::map_segment(loaded.cr3, shim_flag_vaddr, &[0u8], true, false)?;
    loaded.segments.push((shim_flag_vaddr, 1));

    set_current_segments(loaded.segments.clone());
    let pid = spawn(
        name,
        loaded.entry,
        loaded.stack_top,
        loaded.cr3,
        loaded.segments,
    );
    set_current_pid(pid);

    serial_print("Launching agentic: ");
    serial_print(name);
    serial_print("\n");
    fb_print("Launching agentic: ");
    fb_print(name);
    fb_print("\n");

    unsafe {
        jump_to_userspace(loaded.entry, loaded.stack_top, loaded.cr3);
    }
}

unsafe fn jump_to_userspace(entry: u64, stack: u64, cr3: u64) -> ! {
    core::arch::asm!(
        "mov cr3, {cr3}",
        "mov rcx, {entry}",
        "mov r11, 0x202",
        "mov rsp, {stack}",
        "sysretq",
        cr3 = in(reg) cr3,
        entry = in(reg) entry,
        stack = in(reg) stack,
        options(noreturn)
    );
}
