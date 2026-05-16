#![allow(dead_code)]

use alloc::vec::Vec;
use spinning_top::Spinlock;

use super::process::{Process, ProcessClass, ProcessState};
use crate::model::{features, inference, weights};
use crate::{serial_print, serial_print_usize};

const MAX_PAGES: u32 = 65536;
const MAX_CAPS: u16 = 256;

static MODEL: Spinlock<Option<weights::ModelWeights>> = Spinlock::new(None);

pub fn init() {
    let w = weights::load();
    *MODEL.lock() = Some(w);
}

pub fn score(process: &Process, current_tick: u64, max_pages: u32, max_caps: u16) -> f32 {
    let guard = MODEL.lock();
    match guard.as_ref() {
        Some(model) => {
            let features = features::build(process, current_tick, max_pages, max_caps);
            let intent_w = features[12];
            if intent_w > 0.09 {
                serial_print("SCHED: intent boost pid=");
                serial_print_usize(process.pid as usize);
                serial_print(" intent=");
                serial_print_usize(process.stats.last_intent_id as usize);
                serial_print(" weight=");
                serial_print_usize((intent_w * 100.0) as usize);
                serial_print("\n");
            }
            inference::forward(&features, model)
        }
        None => fallback_score(process),
    }
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
        let score = score(process, tick, MAX_PAGES, MAX_CAPS);
        process.priority = score;
    }
}

fn fallback_score(process: &Process) -> f32 {
    match process.class {
        ProcessClass::Realtime => 1.0,
        ProcessClass::Interactive => 0.75,
        ProcessClass::Batch => 0.25,
        ProcessClass::Idle => 0.0,
    }
}
