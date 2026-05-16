#![allow(dead_code)]

use alloc::{format, string::String};

use crate::fs::vfs::FileType;
use crate::{fs, io::audit, memory::pmm, sched::queue};

use super::commands::Intent;
use super::query::collect_snapshot;

pub fn execute(intent: Intent) -> String {
    match intent {
        Intent::SystemStatus => system_status(),
        Intent::MemoryStatus => memory_status(),
        Intent::SchedulerStatus => scheduler_status(),
        Intent::SecurityStatus => security_status(),
        Intent::FileList { path } => file_list(&path),
        Intent::FileRead { path } => file_read(&path),
        Intent::FileWrite { path, content } => file_write(&path, &content),
        Intent::FileCreate { path } => file_create(&path),
        Intent::ProcessList => process_list(),
        Intent::ProcessInfo { pid } => process_info(pid),
        Intent::ExplainScheduler => explain_scheduler(),
        Intent::ExplainMediator => explain_mediator(),
        Intent::ExplainDecision { context } => explain_decision(&context),
        Intent::AskAI { query } => query,
        Intent::Unknown { raw } => format!("I don't understand: {}. Try 'help'.", raw),
    }
}

fn system_status() -> String {
    let snap = collect_snapshot();
    format!(
        "Maya status: {}MB free memory, {} processes, {} ticks uptime, {} security events blocked.",
        snap.free_mb, snap.process_count, snap.tick_count, snap.blocked_events
    )
}

fn memory_status() -> String {
    let s = pmm::stats();
    let free_mb = (s.free_frames * 4096) / (1024 * 1024);
    let used_mb = ((s.total_frames - s.free_frames) * 4096) / (1024 * 1024);
    format!(
        "Memory: {}MB free, {}MB used, {} total frames.",
        free_mb, used_mb, s.total_frames
    )
}

fn scheduler_status() -> String {
    let ticks = queue::tick_count();
    let procs = queue::process_count();
    format!(
        "AI scheduler: PPO policy, 100Hz, {} ticks elapsed, {} processes in queue. Optimization target: balanced latency/throughput.",
        ticks, procs
    )
}

fn security_status() -> String {
    let blocked = audit::total_blocked();
    format!(
        "Security: I/O mediator active, {} events blocked since boot. Anomaly detection: enabled. Capability enforcement: active.",
        blocked
    )
}

fn file_list(path: &str) -> String {
    match fs::open(path) {
        Ok(inode) => match fs::readdir(inode) {
            Ok(entries) => {
                if entries.is_empty() {
                    return format!("{}: empty directory", path);
                }
                let mut result = format!("{}:\n", path);
                for entry in &entries {
                    let kind = match entry.file_type {
                        FileType::Directory => "dir",
                        FileType::Regular => "file",
                    };
                    result.push_str(&format!("  {} [{}]\n", entry.name, kind));
                }
                result
            }
            Err(_) => format!("{}: not a directory", path),
        },
        Err(_) => format!("{}: not found", path),
    }
}

fn file_read(path: &str) -> String {
    match fs::open(path) {
        Ok(inode) => {
            let mut buf = [0u8; 512];
            match fs::read(inode, 0, &mut buf) {
                Ok(n) => {
                    let content = core::str::from_utf8(&buf[..n]).unwrap_or("<binary>");
                    format!("{}:\n{}", path, content)
                }
                Err(_) => format!("{}: read error", path),
            }
        }
        Err(_) => format!("{}: not found", path),
    }
}

fn file_write(path: &str, content: &str) -> String {
    match fs::open(path) {
        Ok(inode) => match fs::write(inode, 0, content.as_bytes()) {
            Ok(n) => format!("Wrote {} bytes to {}", n, path),
            Err(_) => format!("{}: write error", path),
        },
        Err(_) => match fs::create(path, FileType::Regular) {
            Ok(inode) => match fs::write(inode, 0, content.as_bytes()) {
                Ok(n) => format!("Created {} and wrote {} bytes", path, n),
                Err(_) => format!("{}: write error", path),
            },
            Err(_) => format!("{}: could not create", path),
        },
    }
}

fn file_create(path: &str) -> String {
    match fs::create(path, FileType::Directory) {
        Ok(_) => format!("Created directory {}", path),
        Err(_) => format!("{}: already exists or invalid path", path),
    }
}

fn process_list() -> String {
    let count = queue::process_count();
    let ticks = queue::tick_count();
    format!(
        "{} processes running. Scheduler has completed {} ticks. AI policy: PPO balanced mode.",
        count, ticks
    )
}

fn process_info(pid: u16) -> String {
    format!(
        "Process {}: information not yet available in this build.",
        pid
    )
}

fn explain_scheduler() -> String {
    String::from(
        "The Maya scheduler uses a PPO-trained neural network (3-layer MLP, ~500K INT8 params) to assign priority scores to each process every 10ms. It optimizes for low latency on interactive processes and high throughput on batch processes simultaneously, using a 16-dimensional feature vector including CPU usage, I/O wait ratio, IPC rates, and declared process intent."
    )
}

fn explain_mediator() -> String {
    let blocked = audit::total_blocked();
    format!(
        "The I/O mediator intercepts every file, network, and memory operation. It scores each request using 4 anomaly rules: rapid file opens (batch processes), cross-process /proc access, out-of-scope writes, and repeated identical requests. {} events have been blocked since boot.",
        blocked
    )
}

fn explain_decision(context: &str) -> String {
    format!("EXPLAIN: {}", context)
}
