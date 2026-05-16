#![allow(dead_code)]

use alloc::{format, string::String};

use crate::{context, io::audit, memory::pmm, sched::queue};

pub struct SystemSnapshot {
    pub free_frames: usize,
    pub total_frames: usize,
    pub free_mb: usize,
    pub process_count: usize,
    pub tick_count: u64,
    pub blocked_events: usize,
    pub uptime_ticks: u64,
}

pub fn collect_snapshot() -> SystemSnapshot {
    let stats = pmm::stats();
    let free_mb = (stats.free_frames * 4096) / (1024 * 1024);
    let tick_count = queue::tick_count();

    SystemSnapshot {
        free_frames: stats.free_frames,
        total_frames: stats.total_frames,
        free_mb,
        process_count: queue::process_count(),
        tick_count,
        blocked_events: audit::total_blocked(),
        uptime_ticks: tick_count,
    }
}

pub fn build_system_prompt() -> String {
    let snap = collect_snapshot();
    let context = context::snapshot();

    let mut ctx_str = String::new();
    for (key, value) in &context {
        ctx_str.push_str(&format!("- {}: {}\n", key, value));
    }

    format!(
        "You are Maya, an AI-native operating system \
         kernel running on bare-metal x86-64.\n\
         \n\
         Current system state:\n\
         - Free memory: {}MB ({} frames)\n\
         - Total memory: {} frames\n\
         - Active processes: {}\n\
         - System ticks: {}\n\
         - Security events blocked: {}\n\
         - AI scheduler: running (PPO policy, 100Hz)\n\
         - I/O mediator: active\n\
         \n\
         Known context:\n\
         {}\n\
         \n\
         Respond concisely in 1-2 sentences. \
         Be specific about Maya's architecture.",
        snap.free_mb,
        snap.free_frames,
        snap.total_frames,
        snap.process_count,
        snap.tick_count,
        snap.blocked_events,
        ctx_str,
    )
}

pub fn build_query(system_prompt: &str, user_query: &str) -> String {
    format!(
        "MAYA_QUERY_START\n\
         SYSTEM: {}\n\
         USER: {}\n\
         MAYA_QUERY_END\n",
        system_prompt, user_query,
    )
}
