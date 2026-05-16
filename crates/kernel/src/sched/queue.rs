#![allow(dead_code)]

use alloc::vec::Vec;
use spinning_top::Spinlock;

use super::{
    policy,
    process::{Process, ProcessClass, ProcessState},
};

pub struct RunQueue {
    processes: Vec<Process>,
    current_pid: Option<u16>,
    tick_count: u64,
}

struct PerCoreQueue {
    processes: Vec<Process>,
    current_pid: Option<u16>,
    tick_count: u64,
    core_id: u8,
}

const MAX_CORES: usize = 16;

static CORE_QUEUES: [Spinlock<Option<PerCoreQueue>>; MAX_CORES] =
    [const { Spinlock::new(None) }; MAX_CORES];

pub static RUN_QUEUE: Spinlock<RunQueue> = Spinlock::new(RunQueue {
    processes: Vec::new(),
    current_pid: None,
    tick_count: 0,
});

pub fn init() {
    let mut queue = RUN_QUEUE.lock();
    queue.processes.clear();
    queue.current_pid = None;
    queue.tick_count = 0;
    queue.processes.push(Process::new(0, ProcessClass::Idle, 0.0));
}

pub fn init_core(core_id: u8) {
    let mut q = CORE_QUEUES[core_id as usize].lock();
    *q = Some(PerCoreQueue {
        processes: Vec::new(),
        current_pid: None,
        tick_count: 0,
        core_id,
    });
    crate::serial_print("Scheduler: core ");
    crate::serial_print_usize(core_id as usize);
    crate::serial_print(" initialised\n");
}

pub fn add_process(process: Process) {
    RUN_QUEUE.lock().processes.push(process);
}

pub fn add_process_to_core(core_id: u8, process: Process) {
    let mut q = CORE_QUEUES[core_id as usize].lock();
    if let Some(ref mut queue) = *q {
        queue.processes.push(process);
    }
}

pub fn remove_process(pid: u16) {
    let mut queue = RUN_QUEUE.lock();
    queue.processes.retain(|process| process.pid != pid);
    if queue.current_pid == Some(pid) {
        queue.current_pid = None;
    }
}

pub fn current_pid() -> Option<u16> {
    RUN_QUEUE.lock().current_pid
}

pub fn tick_count() -> u64 {
    RUN_QUEUE.lock().tick_count
}

pub fn process_count() -> usize {
    RUN_QUEUE.lock().processes.len()
}

pub fn tick_count_for_core(core_id: u8) -> u64 {
    let q = CORE_QUEUES[core_id as usize].lock();
    q.as_ref().map(|q| q.tick_count).unwrap_or(0)
}

pub fn online_core_count() -> usize {
    CORE_QUEUES.iter().filter(|q| q.lock().is_some()).count()
}

pub fn core_is_online(core_id: u8) -> bool {
    CORE_QUEUES[core_id as usize].lock().is_some()
}

pub fn process_count_for_core(core_id: u8) -> usize {
    CORE_QUEUES[core_id as usize]
        .lock()
        .as_ref()
        .map(|q| q.processes.len())
        .unwrap_or(0)
}

pub fn next_process() -> Option<u16> {
    let mut queue = RUN_QUEUE.lock();
    let current_tick = queue.tick_count;

    for process in &mut queue.processes {
        if process.state == ProcessState::Running {
            process.state = ProcessState::Ready;
        }
    }

    let scores = policy::score_all(&queue.processes, current_tick);
    for (pid, score) in &scores {
        if let Some(process) = queue.processes.iter_mut().find(|process| process.pid == *pid) {
            process.priority = *score;
        }
    }

    let next_pid = scores.into_iter().find_map(|(pid, _score)| {
        queue.processes
            .iter()
            .find(|process| process.pid == pid && process.state == ProcessState::Ready)
            .map(|process| process.pid)
    });

    if let Some(pid) = next_pid {
        if let Some(process) = queue.processes.iter_mut().find(|process| process.pid == pid) {
            process.state = ProcessState::Running;
            process.stats.last_scheduled = current_tick;
        }
    }

    queue.current_pid = next_pid;
    next_pid
}

pub fn tick(current_pid: Option<u16>) {
    let mut queue = RUN_QUEUE.lock();
    queue.tick_count = queue.tick_count.saturating_add(1);
    let current_tick = queue.tick_count;

    for process in &mut queue.processes {
        process.stats.total_ticks_alive = process.stats.total_ticks_alive.saturating_add(1);

        if Some(process.pid) == current_pid {
            process.state = ProcessState::Running;
            process.stats.cpu_ticks_used = process.stats.cpu_ticks_used.saturating_add(1);
            process.stats.burst_ticks = process.stats.burst_ticks.saturating_add(1);
            process.stats.last_scheduled = current_tick;
        } else if process.state == ProcessState::Ready {
            process.stats.ticks_waiting = process.stats.ticks_waiting.saturating_add(1);
            process.stats.burst_ticks = 0;
        }
    }

    drop(queue);
    let _ = next_process();
}

pub fn tick_core(core_id: u8) {
    let mut q = CORE_QUEUES[core_id as usize].lock();
    if let Some(ref mut queue) = *q {
        let _ = queue.current_pid;
        let _ = queue.core_id;
        queue.tick_count = queue.tick_count.saturating_add(1);
        policy::run_policy_for_core(&mut queue.processes, queue.tick_count);
    }
}

pub fn migrate_one_process(from: u8, to: u8) {
    let process = {
        let mut from_q = CORE_QUEUES[from as usize].lock();
        if let Some(ref mut q) = *from_q {
            if q.processes.is_empty() {
                None
            } else {
                Some(q.processes.remove(0))
            }
        } else {
            None
        }
    };

    if let Some(proc) = process {
        let mut to_q = CORE_QUEUES[to as usize].lock();
        if let Some(ref mut q) = *to_q {
            q.processes.push(proc);
        }
    }
}

pub fn update_process_intent(pid: u16, intent_id: u16, tick: u64) {
    for core_id in 0..16u8 {
        if let Some(ref mut queue) = *CORE_QUEUES[core_id as usize].lock() {
            for process in queue.processes.iter_mut() {
                if process.pid == pid {
                    process.stats.last_intent_id = intent_id;
                    process.stats.intent_fire_count =
                        process.stats.intent_fire_count.saturating_add(1);
                    process.stats.intent_fire_tick = tick;
                    return;
                }
            }
        }
    }
}

pub fn get_process(pid: u16) -> Option<Process> {
    RUN_QUEUE
        .lock()
        .processes
        .iter()
        .find(|process| process.pid == pid)
        .cloned()
}

pub fn get_process_mut(_pid: u16) -> Option<&'static mut Process> {
    None
}
