#![allow(dead_code)]

use alloc::vec::Vec;

use crate::cap::table::RawSpinLock;

use super::{
    balance, policy,
    process::{Process, ProcessClass, ProcessState},
};

pub struct PerCoreQueue {
    pub processes: Vec<Process>,
    pub current_pid: Option<u16>,
    pub tick_count: u64,
    pub core_id: u8,
}

const MAX_CORES: usize = 8;

static CORE_QUEUES: [RawSpinLock<Option<PerCoreQueue>>; MAX_CORES] =
    [const { RawSpinLock::new(None) }; MAX_CORES];

pub fn init() {
    init_core(0);
    let mut q = CORE_QUEUES[0].lock();
    if let Some(ref mut queue) = *q {
        queue.processes.clear();
        queue.current_pid = None;
        queue.tick_count = 0;
        queue.processes.push(Process::new(0, ProcessClass::Idle, 0.0));
    }
}

pub fn init_core(core_id: u8) {
    let mut q = CORE_QUEUES[core_id as usize].lock();
    let mut processes = Vec::new();
    if core_id != 0 {
        processes.push(Process::new(core_id as u16, ProcessClass::Idle, 0.0));
    }
    *q = Some(PerCoreQueue {
        processes,
        current_pid: None,
        tick_count: 0,
        core_id,
    });
}

pub fn get_current_core_id() -> u8 {
    crate::arch::cpu::current_core_id()
}

pub fn add_process(process: Process) {
    let core = balance::assign_core_for_new_process();
    add_process_to_core(core, process);
}

pub fn add_process_to_core(core_id: u8, process: Process) {
    let mut q = CORE_QUEUES[core_id as usize].lock();
    if let Some(ref mut queue) = *q {
        queue.processes.push(process);
    }
    drop(q);
    if core_id != 0 {
        crate::arch::gic::send_ipi(core_id as u64);
    }
    unsafe {
        core::arch::asm!("sev", options(nomem, nostack, preserves_flags));
    }
}

pub fn process_count() -> usize {
    let mut total = 0usize;
    for queue in &CORE_QUEUES {
        let guard = queue.lock();
        total += guard.as_ref().map(|q| q.processes.len()).unwrap_or(0);
    }
    total
}

pub fn online_core_count() -> usize {
    CORE_QUEUES.iter().filter(|q| q.lock().is_some()).count()
}

pub fn core_is_online(core_id: u8) -> bool {
    if core_id as usize >= MAX_CORES {
        return false;
    }
    CORE_QUEUES[core_id as usize].lock().is_some()
}

pub fn process_count_for_core(core_id: u8) -> usize {
    if core_id as usize >= MAX_CORES {
        return 0;
    }
    CORE_QUEUES[core_id as usize]
        .lock()
        .as_ref()
        .map(|q| q.processes.len())
        .unwrap_or(0)
}

pub fn tick_count_for_core(core_id: u8) -> u64 {
    if core_id as usize >= MAX_CORES {
        return 0;
    }
    CORE_QUEUES[core_id as usize]
        .lock()
        .as_ref()
        .map(|q| q.tick_count)
        .unwrap_or(0)
}

pub fn tick_handler() {
    let core_id = get_current_core_id();
    let mut q = CORE_QUEUES[core_id as usize].lock();
    let Some(ref mut queue) = *q else {
        return;
    };

    queue.tick_count = queue.tick_count.saturating_add(1);
    let current_tick = queue.tick_count;

    for process in &mut queue.processes {
        process.stats.total_ticks_alive = process.stats.total_ticks_alive.saturating_add(1);
        if Some(process.pid) == queue.current_pid {
            process.state = ProcessState::Running;
            process.stats.cpu_ticks_used = process.stats.cpu_ticks_used.saturating_add(1);
            process.stats.burst_ticks = process.stats.burst_ticks.saturating_add(1);
            process.stats.last_scheduled = current_tick;
        } else if process.state == ProcessState::Ready {
            process.stats.ticks_waiting = process.stats.ticks_waiting.saturating_add(1);
            process.stats.burst_ticks = 0;
        }
    }

    if queue.processes.len() > 1 {
        policy::run_policy_for_core(&mut queue.processes, current_tick);
    }

    let next_pid = queue
        .processes
        .iter_mut()
        .filter(|process| process.state == ProcessState::Ready || process.pid == 0)
        .max_by(|left, right| left.priority.total_cmp(&right.priority))
        .map(|process| {
            process.state = ProcessState::Running;
            process.stats.last_scheduled = current_tick;
            process.pid
        });

    queue.current_pid = next_pid;
    let should_rebalance = core_id == 0 && current_tick % 100 == 0;
    drop(q);

    if should_rebalance {
        balance::rebalance();
    }
}

pub fn next_process() -> Option<u16> {
    let core_id = get_current_core_id();
    let q = CORE_QUEUES[core_id as usize].lock();
    q.as_ref().and_then(|queue| queue.current_pid)
}

pub fn choose_next_process(current_pid: Option<u16>) -> Option<u16> {
    let core_id = get_current_core_id();
    let mut q = CORE_QUEUES[core_id as usize].lock();
    let Some(ref mut queue) = *q else {
        return None;
    };

    queue.tick_count = queue.tick_count.saturating_add(1);
    let tick = queue.tick_count;

    if let Some(pid) = current_pid {
        if let Some(process) = queue.processes.iter_mut().find(|process| process.pid == pid) {
            if process.pid != 0 {
                process.state = ProcessState::Ready;
                process.stats.last_scheduled = tick;
                process.stats.cpu_ticks_used = process.stats.cpu_ticks_used.saturating_add(1);
                process.stats.burst_ticks = process.stats.burst_ticks.saturating_add(1);
            }
        }
    }

    for process in queue.processes.iter_mut() {
        if Some(process.pid) != current_pid && process.pid != 0 && process.state == ProcessState::Ready {
            process.stats.ticks_waiting = process.stats.ticks_waiting.saturating_add(1);
        }
    }

    if queue.processes.len() > 1 {
        policy::run_policy_for_core(&mut queue.processes, tick);
    }

    let mut best: Option<(u16, f32)> = None;
    for process in queue.processes.iter() {
        if process.state != ProcessState::Ready && process.state != ProcessState::Running {
            continue;
        }
        if Some(process.pid) == current_pid {
            continue;
        }
        if process.pid == 0 && queue.processes.len() > 1 {
            continue;
        }
        let starvation_bonus = (process.stats.ticks_waiting as f32).min(50.0) * 0.02;
        let candidate = (process.pid, process.priority + starvation_bonus);
        if best
            .as_ref()
            .is_none_or(|(best_pid, score)| candidate.1 > *score || (candidate.1 == *score && process.pid < *best_pid))
        {
            best = Some(candidate);
        }
    }

    let selected = best.map(|(pid, _)| pid)?;
    if let Some(current_pid) = current_pid {
        if current_pid != selected {
            if let Some(current_process) = queue.processes.iter().find(|process| process.pid == current_pid) {
                policy::compute_and_apply_reward(current_process, core_id as usize);
            }
        }
    }
    if let Some(next_process) = queue.processes.iter().find(|process| process.pid == selected) {
        policy::record_pre_schedule(next_process, core_id as usize, tick);
    }
    for process in queue.processes.iter_mut() {
        if process.pid == selected {
            process.state = ProcessState::Running;
            process.stats.last_scheduled = tick;
            process.stats.ticks_waiting = 0;
            process.stats.burst_ticks = 0;
        } else if process.pid != 0 && process.state == ProcessState::Running {
            process.state = ProcessState::Ready;
        }
    }
    queue.current_pid = Some(selected);
    Some(selected)
}

pub fn core_has_work(core_id: usize) -> bool {
    let q = CORE_QUEUES[core_id].lock();
    let Some(ref queue) = *q else {
        return false;
    };
    queue
        .processes
        .iter()
        .any(|p| p.pid >= 4 && p.state == ProcessState::Ready)
}

pub fn remove_process(pid: u16) {
    for core_id in 0..MAX_CORES as u8 {
        if let Some(ref mut queue) = *CORE_QUEUES[core_id as usize].lock() {
            queue.processes.retain(|process| process.pid != pid);
            if queue.current_pid == Some(pid) {
                queue.current_pid = None;
            }
        }
    }
}

pub fn debug_print_queue() {
    let core_id = get_current_core_id();
    let q = CORE_QUEUES[core_id as usize].lock();
    let Some(ref queue) = *q else {
        crate::uart_print!("Scheduler queue: offline\n");
        return;
    };

    crate::uart_print!("Scheduler queue[");
    crate::uart_print_usize!(core_id as usize);
    crate::uart_print!("] count=");
    crate::uart_print_usize!(queue.processes.len());
    crate::uart_print!("\n");

    let limit = queue.processes.len().min(4);
    for process in queue.processes.iter().take(limit) {
        crate::uart_print!("  pid=");
        crate::uart_print_usize!(process.pid as usize);
        crate::uart_print!(" state=");
        crate::uart_print_usize!(process.state as usize);
        crate::uart_print!(" priority=");
        crate::uart_print_usize!((process.priority * 1000.0) as usize);
        crate::uart_print!("\n");
    }
}

pub fn update_process_intent(pid: u16, intent_id: u16, _tick: u64) {
    let intent_fire_ns = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct());
    unsafe {
        core::arch::asm!(
            "msr daifset, #2",
            options(nomem, nostack, preserves_flags)
        );
    }
    for core_id in 0..MAX_CORES as u8 {
        if let Some(ref mut queue) = *CORE_QUEUES[core_id as usize].lock() {
            for process in queue.processes.iter_mut() {
                if process.pid == pid {
                    process.stats.last_intent_id = intent_id;
                    process.stats.intent_fire_count =
                        process.stats.intent_fire_count.saturating_add(1);
                    process.stats.intent_fire_ns = intent_fire_ns;
                    unsafe {
                        core::arch::asm!(
                            "msr daifclr, #2",
                            options(nomem, nostack, preserves_flags)
                        );
                    }
                    return;
                }
            }
        }
    }
    unsafe {
        core::arch::asm!(
            "msr daifclr, #2",
            options(nomem, nostack, preserves_flags)
        );
    }
}

pub fn update_process_file_stats(pid: u16, reads: u64, writes: u64, bytes_written: u64) -> bool {
    for core_id in 0..MAX_CORES as u8 {
        if let Some(ref mut queue) = *CORE_QUEUES[core_id as usize].lock() {
            for process in queue.processes.iter_mut() {
                if process.pid == pid {
                    process.stats.file_reads = process.stats.file_reads.saturating_add(reads);
                    process.stats.file_writes = process.stats.file_writes.saturating_add(writes);
                    process.stats.file_bytes_written = process
                        .stats
                        .file_bytes_written
                        .saturating_add(bytes_written);
                    return true;
                }
            }
        }
    }
    false
}

pub fn update_process_ipc_stats(pid: u16, sends: u64, recvs: u64) -> bool {
    for core_id in 0..MAX_CORES as u8 {
        if let Some(ref mut queue) = *CORE_QUEUES[core_id as usize].lock() {
            for process in queue.processes.iter_mut() {
                if process.pid == pid {
                    process.stats.ipc_sends = process.stats.ipc_sends.saturating_add(sends);
                    process.stats.ipc_recvs = process.stats.ipc_recvs.saturating_add(recvs);
                    return true;
                }
            }
        }
    }
    false
}

#[inline(always)]
fn casal_add_u64(ptr: *mut u64, delta: u64) {
    let mut current = unsafe { core::ptr::read_volatile(ptr) };
    loop {
        let desired = current.saturating_add(delta);
        let mut observed = current;
        unsafe {
            core::arch::asm!(
                "casal {old}, {new}, [{ptr}]",
                old = inout(reg) observed,
                new = in(reg) desired,
                ptr = in(reg) ptr,
                options(nostack)
            );
        }
        if observed == current {
            break;
        }
        current = observed;
    }
}

pub fn update_process_alarm_stats(pid: u16, alarms_sent: u64, alarms_acked: u64) -> bool {
    for core_id in 0..MAX_CORES as u8 {
        if let Some(ref mut queue) = *CORE_QUEUES[core_id as usize].lock() {
            for process in queue.processes.iter_mut() {
                if process.pid == pid {
                    if alarms_sent > 0 {
                        casal_add_u64(core::ptr::addr_of_mut!(process.stats.alarms_sent), alarms_sent);
                    }
                    if alarms_acked > 0 {
                        casal_add_u64(core::ptr::addr_of_mut!(process.stats.alarms_acked), alarms_acked);
                    }
                    return true;
                }
            }
        }
    }
    false
}

pub fn migrate_one_process(from: u8, to: u8) {
    let process = {
        let mut from_q = CORE_QUEUES[from as usize].lock();
        if let Some(ref mut q) = *from_q {
            if q.processes.len() <= 1 {
                None
            } else {
                Some(q.processes.remove(0))
            }
        } else {
            None
        }
    };

    if let Some(proc) = process {
        add_process_to_core(to, proc);
    }
}

pub fn get_process(pid: u16) -> Option<Process> {
    for core_id in 0..MAX_CORES as u8 {
        let q = CORE_QUEUES[core_id as usize].lock();
        if let Some(ref queue) = *q {
            if let Some(process) = queue.processes.iter().find(|process| process.pid == pid) {
                return Some(process.clone());
            }
        }
    }
    None
}

pub fn get_process_with_core(pid: u16) -> Option<(u8, Process)> {
    for core_id in 0..MAX_CORES as u8 {
        let q = CORE_QUEUES[core_id as usize].lock();
        if let Some(ref queue) = *q {
            if let Some(process) = queue.processes.iter().find(|process| process.pid == pid) {
                return Some((core_id, process.clone()));
            }
        }
    }
    None
}

pub fn get_process_with_core_try(pid: u16) -> Option<(u8, Process)> {
    for core_id in 0..MAX_CORES as u8 {
        let Some(q) = CORE_QUEUES[core_id as usize].try_lock() else {
            continue;
        };
        if let Some(ref queue) = *q {
            if let Some(process) = queue.processes.iter().find(|process| process.pid == pid) {
                return Some((core_id, process.clone()));
            }
        }
    }
    None
}

pub fn set_process_intent_class(pid: u16, intent_class: crate::cap::IntentClass) -> bool {
    use crate::cap::IntentClass;

    let new_class = match intent_class {
        IntentClass::RealTime => ProcessClass::Realtime,
        IntentClass::Compute | IntentClass::IO | IntentClass::System => ProcessClass::Interactive,
        IntentClass::Background | IntentClass::Unknown => ProcessClass::Batch,
    };

    for core_id in 0..MAX_CORES as u8 {
        if let Some(ref mut queue) = *CORE_QUEUES[core_id as usize].lock() {
            if let Some(process) = queue.processes.iter_mut().find(|process| process.pid == pid) {
                process.intent_class = intent_class;
                process.class = new_class;
                return true;
            }
        }
    }
    false
}
