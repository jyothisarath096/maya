#![allow(dead_code)]

use crate::sched::process::Process;

pub fn build(
    process: &Process,
    current_tick: u64,
    max_pages: u32,
    max_caps: u16,
) -> [f32; 16] {
    process.to_features(current_tick, max_pages, max_caps)
}
