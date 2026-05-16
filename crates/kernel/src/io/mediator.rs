#![allow(dead_code)]

use alloc::{
    boxed::Box,
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use spinning_top::Spinlock;

use crate::{
    io::{
        audit::{self, IoEvent, IoEventKind, MediatorDecision},
        syscall::IoRequest,
    },
    sched::{
        process::ProcessClass,
        queue,
    },
};

pub struct MediationResult {
    pub decision: MediatorDecision,
    pub reason: &'static str,
    pub latency_ns: u64,
}

#[derive(Clone)]
struct RequestRecord {
    tick: u64,
    pid: u16,
    kind: IoEventKind,
    path: Option<String>,
}

struct MediatorState {
    scopes: BTreeMap<u16, String>,
    history: Vec<RequestRecord>,
}

static MEDIATOR: Spinlock<Option<Box<MediatorState>>> = Spinlock::new(None);

pub fn declare_scope(pid: u16, path: &str) {
    let mut guard = MEDIATOR.lock();
    let state = guard.get_or_insert_with(|| {
        Box::new(MediatorState {
            scopes: BTreeMap::new(),
            history: Vec::new(),
        })
    });
    state.scopes.insert(pid, normalize_scope(path));
}

pub fn mediate(pid: u16, request: &IoRequest) -> MediationResult {
    let start_tick = queue::tick_count();
    let (decision, reason) = if pid < 4 {
        (MediatorDecision::Allow, "kernel process")
    } else {
        let process_class = queue::get_process(pid)
            .map(|process| process.class)
            .unwrap_or(ProcessClass::Batch);

        if process_class == ProcessClass::Realtime {
            (MediatorDecision::Allow, "realtime process")
        } else {
            let mut score = 0.0f32;
            let current_tick = start_tick;

            {
                let mut guard = MEDIATOR.lock();
                let state = guard.get_or_insert_with(|| {
                    Box::new(MediatorState {
                        scopes: BTreeMap::new(),
                        history: Vec::new(),
                    })
                });

                let recent_file_opens = state
                    .history
                    .iter()
                    .filter(|record| {
                        record.pid == pid
                            && matches!(record.kind, IoEventKind::FileOpen)
                            && current_tick.saturating_sub(record.tick) <= 10
                    })
                    .count();
                if process_class == ProcessClass::Batch && recent_file_opens > 50 {
                    score += 0.4;
                }

                if accesses_other_proc(pid, request) {
                    score += 0.6;
                }

                if matches!(request.kind, IoEventKind::FileWrite)
                    && !path_in_scope(pid, request, &state.scopes)
                {
                    score += 0.5;
                }

                let identical_requests = state
                    .history
                    .iter()
                    .filter(|record| {
                        record.pid == pid
                            && current_tick.saturating_sub(record.tick) <= 10
                            && record.kind == request.kind
                            && record.path.as_deref() == request.path.as_deref()
                    })
                    .count();
                if identical_requests > 20 {
                    score += 0.3;
                }

                state.history.push(RequestRecord {
                    tick: current_tick,
                    pid,
                    kind: request.kind,
                    path: request.path.clone(),
                });
                if state.history.len() > 256 {
                    state.history.remove(0);
                }
            }

            if score == 0.0 {
                (MediatorDecision::Allow, "normal")
            } else if score < 0.5 {
                (MediatorDecision::Flag, "suspicious")
            } else {
                (MediatorDecision::Block, "anomaly detected")
            }
        }
    };

    let end_tick = queue::tick_count();
    let latency_ns = end_tick.saturating_sub(start_tick).saturating_mul(10_000_000);

    audit::log(IoEvent {
        tick: end_tick,
        pid,
        kind: request.kind,
        resource_id: resource_id_for(request),
        decision,
    });

    MediationResult {
        decision,
        reason,
        latency_ns,
    }
}

fn normalize_scope(path: &str) -> String {
    if path.ends_with('/') {
        path.to_string()
    } else {
        let mut normalized = path.to_string();
        normalized.push('/');
        normalized
    }
}

fn path_in_scope(pid: u16, request: &IoRequest, scopes: &BTreeMap<u16, String>) -> bool {
    let Some(path) = request.path.as_deref() else {
        return true;
    };

    if path == "/proc/self" || path.starts_with("/proc/self/") {
        return true;
    }

    if let Some(scope) = scopes.get(&pid) {
        return path.starts_with(scope.as_str());
    }

    false
}

fn accesses_other_proc(pid: u16, request: &IoRequest) -> bool {
    let Some(path) = request.path.as_deref() else {
        return false;
    };

    if !path.starts_with("/proc/") {
        return false;
    }

    let remainder = &path["/proc/".len()..];
    if remainder.starts_with("self") {
        return false;
    }

    let target = remainder.split('/').next().unwrap_or("");
    match target.parse::<u16>() {
        Ok(other_pid) => other_pid != pid,
        Err(_) => false,
    }
}

fn resource_id_for(request: &IoRequest) -> u32 {
    let mut hash = match request.kind {
        IoEventKind::FileOpen => 1u32,
        IoEventKind::FileRead => 2,
        IoEventKind::FileWrite => 3,
        IoEventKind::FileCreate => 4,
        IoEventKind::FileUnlink => 5,
        IoEventKind::NetworkSend => 6,
        IoEventKind::NetworkRecv => 7,
        IoEventKind::MemoryMap => 8,
    };

    if let Some(path) = request.path.as_deref() {
        for byte in path.bytes() {
            hash = hash.wrapping_mul(16777619) ^ u32::from(byte);
        }
    }

    hash
}
