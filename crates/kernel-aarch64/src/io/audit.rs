#![allow(dead_code)]

use alloc::vec::Vec;

use crate::{cap::IntentClass, cap::table::RawSpinLock};

use super::syscall::IoEventKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediatorDecision {
    Allow,
    Block,
    Flag,
}

#[derive(Debug, Clone, Copy)]
pub struct IoEvent {
    pub tick: u64,
    pub pid: u16,
    pub kind: IoEventKind,
    pub resource_id: u32,
    pub decision: MediatorDecision,
    pub latency_ns: u64,
    pub intent_class: IntentClass,
    pub anomaly_score: f32,
}

const AUDIT_SIZE: usize = 1024;
const EMPTY_EVENT: IoEvent = IoEvent {
    tick: 0,
    pid: 0,
    kind: IoEventKind::FileOpen,
    resource_id: 0,
    decision: MediatorDecision::Allow,
    latency_ns: 0,
    intent_class: IntentClass::Unknown,
    anomaly_score: 0.0,
};

struct AuditLog {
    events: [IoEvent; AUDIT_SIZE],
    head: usize,
    count: usize,
}

static AUDIT: RawSpinLock<AuditLog> = RawSpinLock::new(AuditLog {
    events: [EMPTY_EVENT; AUDIT_SIZE],
    head: 0,
    count: 0,
});

pub fn log(event: IoEvent) {
    let mut audit = AUDIT.lock();
    let head = audit.head;
    audit.events[head] = event;
    audit.head = (head + 1) % AUDIT_SIZE;
    if audit.count < AUDIT_SIZE {
        audit.count += 1;
    }
}

pub fn recent(n: usize) -> Vec<IoEvent> {
    let audit = AUDIT.lock();
    let take = n.min(audit.count);
    let start = (audit.head + AUDIT_SIZE - take) % AUDIT_SIZE;
    let mut events = Vec::with_capacity(take);

    for index in 0..take {
        let event_index = (start + index) % AUDIT_SIZE;
        events.push(audit.events[event_index]);
    }

    events
}

pub fn count_blocked(pid: u16) -> usize {
    let audit = AUDIT.lock();
    let mut blocked = 0;

    for index in 0..audit.count {
        let event_index = (audit.head + AUDIT_SIZE - audit.count + index) % AUDIT_SIZE;
        let event = audit.events[event_index];
        if event.pid == pid && event.decision == MediatorDecision::Block {
            blocked += 1;
        }
    }

    blocked
}

pub fn total_blocked() -> usize {
    let audit = AUDIT.lock();
    let count = audit.count.min(AUDIT_SIZE);
    audit.events[..count]
        .iter()
        .filter(|event| matches!(event.decision, MediatorDecision::Block))
        .count()
}
