#![allow(dead_code)]

use core::sync::atomic::{AtomicI32, Ordering};

use alloc::vec::Vec;

use super::process::{Process, ProcessClass, ProcessState};
use crate::model::{features, inference, weights};

const MAX_PAGES: u32 = 65536;
const MAX_CAPS: u16 = 256;
const MAX_CORES: usize = 8;
const SNAP_DEPTH: usize = 8;

#[derive(Clone, Copy)]
struct ProcessSnapshot {
    pid: u16,
    intent_fire_count: u64,
    ipc_sends: u64,
    ipc_recvs: u64,
    file_reads: u64,
    file_writes: u64,
    ticks_waiting: u64,
    class: u8,
    predicted_score: f32,
    h2: [f32; 64],
}

impl ProcessSnapshot {
    const EMPTY: Self = Self {
        pid: 0,
        intent_fire_count: 0,
        ipc_sends: 0,
        ipc_recvs: 0,
        file_reads: 0,
        file_writes: 0,
        ticks_waiting: 0,
        class: 0,
        predicted_score: 0.0,
        h2: [0.0; 64],
    };
}

static mut SNAPSHOTS: [[ProcessSnapshot; SNAP_DEPTH]; MAX_CORES] =
    [[ProcessSnapshot::EMPTY; SNAP_DEPTH]; MAX_CORES];
static mut SNAP_HEAD: [usize; MAX_CORES] = [0; MAX_CORES];
static mut UPDATE_COUNTER: [u32; MAX_CORES] = [0; MAX_CORES];
static LAST_REWARDS: [AtomicI32; MAX_CORES] = [const { AtomicI32::new(0) }; MAX_CORES];
static BASELINE: [AtomicI32; MAX_CORES] = [const { AtomicI32::new(0) }; MAX_CORES];

fn casal_store_i32(dst: &AtomicI32, new_value: i32) {
    let ptr = dst.as_ptr();
    let mut current = dst.load(Ordering::Relaxed);
    loop {
        let mut observed = current;
        unsafe {
            core::arch::asm!(
                "casal {old:w}, {new:w}, [{ptr}]",
                old = inout(reg) observed,
                new = in(reg) new_value,
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

fn update_baseline(core_id: usize, reward_scaled: i32) -> i32 {
    let current = BASELINE[core_id].load(Ordering::Relaxed);
    let next = ((current as i64 * 800) + (reward_scaled as i64 * 200)) / 1000;
    let next = next.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    casal_store_i32(&BASELINE[core_id], next);
    next
}

fn uart_print_i32(value: i32) {
    if value < 0 {
        crate::uart_print!("-");
        crate::uart_print_usize!(value.unsigned_abs() as usize);
    } else {
        crate::uart_print_usize!(value as usize);
    }
}

pub fn init() {}

pub fn get_last_rewards() -> [i32; MAX_CORES] {
    core::array::from_fn(|i| LAST_REWARDS[i].load(Ordering::Relaxed))
}

pub fn score(process: &Process, current_tick: u64, max_pages: u32, max_caps: u16) -> f32 {
    if weights::all_zero() {
        return fallback_score(process.class);
    }

    let features = features::build(process, current_tick, max_pages, max_caps);
    let model = weights::load();
    inference::forward(&features, &model)
}

pub fn score_all(processes: &[Process], current_tick: u64) -> Vec<(u16, f32)> {
    let mut scores: Vec<(u16, f32)> = processes
        .iter()
        .filter(|process| process.state == ProcessState::Ready)
        .map(|process| (process.pid, score(process, current_tick, MAX_PAGES, MAX_CAPS)))
        .collect();

    scores.sort_by(|left, right| right.1.total_cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    scores
}

pub fn run_policy_for_core(processes: &mut Vec<Process>, tick: u64) {
    for process in processes.iter_mut() {
        let class_val = process.class as u8;
        if class_val > 3 {
            continue;
        }

        if process.class == ProcessClass::Realtime {
            process.priority = 2.0;
            continue;
        }

        process.priority = if weights::all_zero() {
            fallback_score(process.class)
        } else {
            score(process, tick, MAX_PAGES, MAX_CAPS)
        };
    }
}

pub fn record_pre_schedule(process: &Process, core_id: usize, tick: u64) {
    if core_id >= MAX_CORES {
        return;
    }

    let features = features::build(process, tick, MAX_PAGES, MAX_CAPS);
    let model = weights::load();
    let (predicted, h2) = inference::forward_with_activations(&features, &model);

    unsafe {
        let head = SNAP_HEAD[core_id];
        SNAPSHOTS[core_id][head % SNAP_DEPTH] = ProcessSnapshot {
            pid: process.pid,
            intent_fire_count: process.stats.intent_fire_count,
            ipc_sends: process.stats.ipc_sends,
            ipc_recvs: process.stats.ipc_recvs,
            file_reads: process.stats.file_reads,
            file_writes: process.stats.file_writes,
            ticks_waiting: process.stats.ticks_waiting,
            class: process.class as u8,
            predicted_score: predicted,
            h2,
        };
        SNAP_HEAD[core_id] = head.wrapping_add(1);
    }
}

pub fn compute_and_apply_reward(process: &Process, core_id: usize) {
    if core_id >= MAX_CORES {
        return;
    }

    let snap = unsafe {
        let head = SNAP_HEAD[core_id];
        let mut found = None;
        for i in 0..SNAP_DEPTH {
            let idx = head.wrapping_sub(1 + i) % SNAP_DEPTH;
            let candidate = SNAPSHOTS[core_id][idx];
            if candidate.pid == process.pid {
                found = Some(candidate);
                break;
            }
        }
        found
    };

    let Some(snap) = snap else {
        return;
    };

    let mut reward = 0.0f32;

    let intent_delta = process
        .stats
        .intent_fire_count
        .saturating_sub(snap.intent_fire_count);
    if intent_delta > 0 {
        reward += 0.3_f32.min(intent_delta as f32 * 0.1);
    }

    let ipc_delta = process.stats.ipc_sends.saturating_sub(snap.ipc_sends)
        + process.stats.ipc_recvs.saturating_sub(snap.ipc_recvs);
    if ipc_delta > 0 {
        reward += 0.2_f32.min(ipc_delta as f32 * 0.05);
    }

    let file_delta = process.stats.file_writes.saturating_sub(snap.file_writes)
        + process.stats.file_reads.saturating_sub(snap.file_reads);
    if file_delta > 0 {
        reward += 0.1_f32.min(file_delta as f32 * 0.05);
    }

    if snap.class == ProcessClass::Realtime as u8 && snap.ticks_waiting > 2 {
        reward -= 0.3;
    }

    if snap.ticks_waiting > 10 {
        let excess = (snap.ticks_waiting - 10) as f32;
        reward -= (0.2 * excess / 50.0).min(0.3);
    }

    let reward = reward.clamp(-1.0, 1.0);
    let reward_scaled = (reward * 1000.0) as i32;
    let baseline = BASELINE[core_id].load(Ordering::Relaxed);
    let advantage = reward_scaled - baseline;
    let _ = update_baseline(core_id, reward_scaled);
    LAST_REWARDS[core_id].store((reward * 100.0) as i32, Ordering::Relaxed);

    unsafe {
        UPDATE_COUNTER[core_id] = UPDATE_COUNTER[core_id].wrapping_add(1);
        if core_id == 0 && UPDATE_COUNTER[core_id] % 64 == 0 {
            crate::uart_print!("ADV r=");
            uart_print_i32(reward_scaled);
            crate::uart_print!(" b=");
            uart_print_i32(baseline);
            crate::uart_print!(" a=");
            uart_print_i32(advantage);
            crate::uart_print!("\n");
        }
        if UPDATE_COUNTER[core_id] % 16 == 0 && advantage != 0 {
            weights::update_output_weights(&snap.h2, advantage);
        }
    }
}

fn fallback_score(class: ProcessClass) -> f32 {
    match class {
        ProcessClass::Realtime => 1.0,
        ProcessClass::Interactive => 0.75,
        ProcessClass::Batch => 0.25,
        ProcessClass::Idle => 0.0,
    }
}
