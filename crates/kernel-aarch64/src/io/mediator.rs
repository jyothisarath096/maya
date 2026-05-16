#![allow(dead_code)]

use crate::{
    cap::{self, IntentClass, Rights},
    cap::table::RawSpinLock,
    io::{
        audit::{self, IoEvent, MediatorDecision},
        syscall::{IoEventKind, IoRequest},
    },
    proc,
    sched::queue,
    KernelError,
};

const MAX_SCOPES: usize = 16;
const MAX_HISTORY: usize = 64;

const EMPTY_SCOPE: ScopeEntry = ScopeEntry {
    pid: 0,
    valid: false,
    path: [0; 64],
    path_len: 0,
};

const EMPTY_RECORD: RequestRecord = RequestRecord {
    tick: 0,
    pid: 0,
    kind: IoEventKind::FileOpen,
    path: [0; 64],
    path_len: 0,
    valid: false,
};

pub struct MediationResult {
    pub decision: MediatorDecision,
    pub reason: &'static str,
    pub latency_ns: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct ScopeEntry {
    pid: u16,
    valid: bool,
    path: [u8; 64],
    path_len: usize,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct RequestRecord {
    tick: u64,
    pid: u16,
    kind: IoEventKind,
    path: [u8; 64],
    path_len: usize,
    valid: bool,
}

struct MediatorState {
    scopes: [ScopeEntry; MAX_SCOPES],
    history: [RequestRecord; MAX_HISTORY],
    hist_head: usize,
    hist_count: usize,
    fuzz_mode: bool,
}

const EMPTY_STATE: MediatorState = MediatorState {
    scopes: [EMPTY_SCOPE; MAX_SCOPES],
    history: [EMPTY_RECORD; MAX_HISTORY],
    hist_head: 0,
    hist_count: 0,
    fuzz_mode: false,
};

static MEDIATOR: RawSpinLock<MediatorState> = RawSpinLock::new(EMPTY_STATE);
static LAST_ANOMALY: RawSpinLock<[u8; 256]> = RawSpinLock::new([0u8; 256]);

pub fn init() {
    crate::kdbg!("MediatorState size=", core::mem::size_of::<MediatorState>());
    crate::kdbg!("ScopeEntry size=", core::mem::size_of::<ScopeEntry>());
    crate::kdbg!(
        "RequestRecord size=",
        core::mem::size_of::<RequestRecord>()
    );
    crate::kdbg!("MEDIATOR addr=", core::ptr::addr_of!(MEDIATOR) as usize);
    reset();
}

#[inline(never)]
pub fn reset() {
    let mut state = MEDIATOR.lock();
    state.scopes = [EMPTY_SCOPE; MAX_SCOPES];
    state.history = [EMPTY_RECORD; MAX_HISTORY];
    state.hist_head = 0;
    state.hist_count = 0;
    state.fuzz_mode = false;
}

#[inline(never)]
pub fn set_fuzz_mode(enabled: bool) {
    MEDIATOR.lock().fuzz_mode = enabled;
}

pub fn record_anomaly_score(pid: u16, score: f32) {
    let idx = (pid as usize) % 256;
    let mut scores = LAST_ANOMALY.lock();
    scores[idx] = (score * 100.0).min(255.0) as u8;
}

pub fn last_anomaly_score(pid: u16) -> u8 {
    let idx = (pid as usize) % 256;
    let scores = LAST_ANOMALY.lock();
    scores[idx]
}

#[inline(never)]
pub fn declare_scope(pid: u16, path_bytes: &[u8], len: usize) {
    use core::mem::MaybeUninit;

    crate::kdbg!("declare_scope: acquiring lock");
    let copy_len = len.min(63);
    let mut local_path: MaybeUninit<[u8; 64]> = MaybeUninit::uninit();
    let local_path_ptr = local_path.as_mut_ptr() as *mut u8;
    for i in 0..64usize {
        unsafe {
            core::ptr::write_volatile(local_path_ptr.add(i), 0u8);
        }
    }
    let mut local_path = unsafe { local_path.assume_init() };
    for i in 0..copy_len {
        unsafe {
            core::ptr::write_volatile(&mut local_path[i], path_bytes[i]);
        }
    }
    if copy_len > 0 && local_path[copy_len - 1] != b'/' {
        local_path[copy_len] = b'/';
    }
    let final_len = if copy_len > 0 && path_bytes[copy_len - 1] != b'/' {
        copy_len + 1
    } else {
        copy_len
    };

    let mut state = MEDIATOR.lock();
    crate::kdbg!("declare_scope: lock acquired");
    let slot = find_scope_for_pid(&state, pid).or_else(|| find_free_scope(&mut state));
    crate::kdbg!("declare_scope: slot search done");
    if let Some(i) = slot {
        crate::kdbg!("declare_scope: writing pid");
        state.scopes[i].pid = pid;
        crate::kdbg!("declare_scope: writing path_len");
        state.scopes[i].path_len = final_len;
        crate::kdbg!("declare_scope: copy_nonoverlapping");
        unsafe {
            core::ptr::copy_nonoverlapping(
                local_path.as_ptr(),
                state.scopes[i].path.as_mut_ptr(),
                64,
            );
            crate::kdbg!("declare_scope: dmb sy");
            core::arch::asm!("dmb sy", options(nomem, nostack, preserves_flags));
        }
        crate::kdbg!("declare_scope: marking valid");
        state.scopes[i].valid = true;
        crate::kdbg!("declare_scope: done");
    } else {
        crate::kdbg!("declare_scope: no free slot");
    }
}

#[inline(never)]
pub fn declare_scope_unchecked(pid: u16, path: &str) {
    crate::kdbg!("declare_scope_unchecked: entry");
    crate::kdbg!("declare_scope_unchecked: computing len");
    let len = path.len();
    crate::kdbg!("declare_scope_unchecked: calling declare_scope");
    declare_scope(pid, path.as_bytes(), len);
    crate::kdbg!("declare_scope_unchecked: returned");
}

#[inline(never)]
pub fn mediate(pid: u16, request: &IoRequest) -> MediationResult {
    let _ = pid;
    crate::kdbg!("mediate: entry");
    let start_ns = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct());

    if matches!(request.kind, IoEventKind::MemoryMap) {
        mmio_barrier();
    }

    let fuzz_mode = MEDIATOR.lock().fuzz_mode;
    let intent_class = match check_io_capability(pid, request, fuzz_mode) {
        Ok(class) => {
            crate::kdbg!("mediate: capability ok");
            class
        }
        Err(_) => {
            return MediationResult {
                decision: MediatorDecision::Block,
                reason: "no capability",
                latency_ns: crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct())
                    .saturating_sub(start_ns),
            };
        }
    };
    crate::kdbg!("mediate: intent_class=", intent_class as u16 as usize);

    if pid < 4 {
        record_anomaly_score(pid, 0.0);
        let latency_ns = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct())
            .saturating_sub(start_ns);
        audit::log(IoEvent {
            tick: queue::tick_count_for_core(queue::get_current_core_id()),
            pid,
            kind: request.kind,
            resource_id: resource_id_for(request),
            decision: MediatorDecision::Allow,
            latency_ns,
            intent_class,
            anomaly_score: 0.0,
        });
        return MediationResult {
            decision: MediatorDecision::Allow,
            reason: "kernel process",
            latency_ns,
        };
    }

    let process_intent_class = proc::get_process_intent_class(pid);
    if process_intent_class == IntentClass::RealTime {
        record_anomaly_score(pid, 0.0);
        let latency_ns = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct())
            .saturating_sub(start_ns);
        audit::log(IoEvent {
            tick: queue::tick_count_for_core(queue::get_current_core_id()),
            pid,
            kind: request.kind,
            resource_id: resource_id_for(request),
            decision: MediatorDecision::Allow,
            latency_ns,
            intent_class,
            anomaly_score: 0.0,
        });
        return MediationResult {
            decision: MediatorDecision::Allow,
            reason: "realtime process",
            latency_ns,
        };
    }

    let current_tick = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct());
    let score = {
        let mut state = MEDIATOR.lock();
        let score = anomaly_score(pid, request, &state, intent_class, current_tick);
        push_history(&mut state, pid, request, current_tick);
        score
    };
    crate::kdbg!("anomaly score=", (score * 100.0) as usize);

    let decision = if score == 0.0 {
        MediatorDecision::Allow
    } else if score < 0.5 {
        MediatorDecision::Flag
    } else {
        MediatorDecision::Block
    };

    if matches!(decision, MediatorDecision::Block | MediatorDecision::Flag) {
        scheduler_feedback(pid, request);
    }
    record_anomaly_score(pid, score);

    let latency_ns = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct())
        .saturating_sub(start_ns);
    audit::log(IoEvent {
        tick: current_tick,
        pid,
        kind: request.kind,
        resource_id: resource_id_for(request),
        decision,
        latency_ns,
        intent_class,
        anomaly_score: score,
    });

    MediationResult {
        decision,
        reason: match decision {
            MediatorDecision::Allow => "normal",
            MediatorDecision::Flag => "suspicious",
            MediatorDecision::Block => "anomaly detected",
        },
        latency_ns,
    }
}

fn mmio_barrier() {
    unsafe {
        core::arch::asm!(
            "dsb sy",
            "isb",
            options(nomem, nostack, preserves_flags)
        );
    }
}

fn validate_user_path(path: &[u8; 64], len: usize) -> bool {
    len <= 64
        && core::str::from_utf8(&path[..len]).is_ok()
        && path[..len].iter().all(|&b| b != 0 || true)
}

fn check_io_capability(
    pid: u16,
    request: &IoRequest,
    fuzz_mode: bool,
) -> Result<IntentClass, KernelError> {
    if let Some(path) = request.path.as_ref() {
        if !validate_user_path(path, request.path_len) {
            return Err(KernelError::CapInvalidToken);
        }
    }

    if fuzz_mode {
        return Ok(IntentClass::Unknown);
    }

    let Some(token) = request.cap_token else {
        if pid < 4 {
            return Ok(IntentClass::System);
        }
        if matches!(request.kind, IoEventKind::FileOpen | IoEventKind::FileCreate) {
            return Ok(proc::get_process_intent_class(pid));
        }
        return Err(KernelError::CapInvalidToken);
    };

    cap::check_right_as(token, required_right(request.kind), pid)?;
    cap::get_intent_class(token)
}

fn required_right(kind: IoEventKind) -> Rights {
    match kind {
        IoEventKind::FileRead | IoEventKind::FileOpen => Rights::READ,
        IoEventKind::FileWrite | IoEventKind::FileCreate => Rights::WRITE,
        IoEventKind::FileUnlink => Rights::REVOKE,
        IoEventKind::NetworkSend => Rights::INTENT_CALL,
        IoEventKind::NetworkRecv => Rights::OBSERVE,
        IoEventKind::MemoryMap => Rights::EXECUTE,
    }
}

#[inline(never)]
fn anomaly_score(
    pid: u16,
    request: &IoRequest,
    state: &MediatorState,
    intent_class: IntentClass,
    current_tick: u64,
) -> f32 {
    let mut score = 0.0f32;

    let recent_opens = count_recent(
        state,
        pid,
        IoEventKind::FileOpen,
        current_tick,
        500_000_000,
    );
    if recent_opens > 50 {
        score += 0.4;
    }

    if accesses_other_proc(pid, request) {
        score += 0.6;
    }

    if matches!(request.kind, IoEventKind::FileWrite) && !path_in_scope(pid, request, &state.scopes)
    {
        score += 0.5;
    }

    let identical = count_identical(state, pid, request, current_tick, 500_000_000);
    crate::kdbg!("anomaly: hist_count=", state.hist_count);
    crate::kdbg!("anomaly: identical=", identical);
    if identical > 20 {
        score += 0.3;
    }

    let class_discount = match intent_class {
        IntentClass::IO => 0.3,
        IntentClass::Compute => 0.1,
        IntentClass::System => 0.5,
        IntentClass::RealTime => 0.2,
        IntentClass::Unknown | IntentClass::Background => 0.0,
    };

    let kind_discount = match (intent_class, request.kind) {
        (IntentClass::IO, IoEventKind::NetworkSend) => 0.2,
        (IntentClass::IO, IoEventKind::NetworkRecv) => 0.2,
        (IntentClass::Compute, IoEventKind::MemoryMap) => 0.15,
        _ => 0.0,
    };

    let final_score = (score - class_discount - kind_discount).max(0.0);
    crate::kdbg!(
        "anomaly: class_discount=",
        (class_discount * 100.0) as usize
    );
    crate::kdbg!("anomaly: raw_score=", (score * 100.0) as usize);
    crate::kdbg!("anomaly: final_score=", (final_score * 100.0) as usize);
    final_score
}

fn count_recent(
    state: &MediatorState,
    pid: u16,
    kind: IoEventKind,
    current_tick: u64,
    window: u64,
) -> usize {
    iter_history(state)
        .filter(|record| {
            record.pid == pid
                && record.kind == kind
                && current_tick.saturating_sub(record.tick) <= window
        })
        .count()
}

fn count_identical(
    state: &MediatorState,
    pid: u16,
    request: &IoRequest,
    current_tick: u64,
    window: u64,
) -> usize {
    let request_path = request.path.as_ref().map(|path| &path[..request.path_len.min(64)]);
    iter_history(state)
        .filter(|record| {
            record.pid == pid
                && current_tick.saturating_sub(record.tick) <= window
                && record.kind == request.kind
                && match request_path {
                    Some(path) => {
                        record.path_len == request.path_len.min(64)
                            && record.path[..record.path_len.min(64)] == path[..request.path_len.min(64)]
                    }
                    None => record.path_len == 0,
                }
        })
        .count()
}

fn iter_history(state: &MediatorState) -> impl Iterator<Item = &RequestRecord> {
    state.history.iter().filter(|record| record.valid)
}

#[inline(never)]
fn push_history(state: &mut MediatorState, pid: u16, request: &IoRequest, tick: u64) {
    use core::mem::MaybeUninit;

    crate::kdbg!("push_history: pid=", pid as usize);
    let slot = state.hist_head;
    let path = if let Some(path) = request.path {
        path
    } else {
        let mut zeroed: MaybeUninit<[u8; 64]> = MaybeUninit::uninit();
        let zeroed_ptr = zeroed.as_mut_ptr() as *mut u8;
        for i in 0..64usize {
            unsafe {
                core::ptr::write_volatile(zeroed_ptr.add(i), 0u8);
            }
        }
        unsafe { zeroed.assume_init() }
    };
    let path_len = request.path_len.min(64);
    state.history[slot].valid = false;
    state.history[slot].pid = pid;
    state.history[slot].kind = request.kind;
    state.history[slot].tick = tick;
    state.history[slot].path_len = path_len;
    unsafe {
        core::ptr::copy_nonoverlapping(
            path.as_ptr(),
            state.history[slot].path.as_mut_ptr(),
            64,
        );
        core::arch::asm!("dmb sy", options(nomem, nostack, preserves_flags));
    }
    state.history[slot].valid = true;
    state.hist_head = (state.hist_head + 1) % MAX_HISTORY;
    if state.hist_count < MAX_HISTORY {
        state.hist_count += 1;
    }
}

#[inline(never)]
fn find_scope_for_pid(state: &MediatorState, pid: u16) -> Option<usize> {
    for i in 0..MAX_SCOPES {
        if state.scopes[i].valid && state.scopes[i].pid == pid {
            return Some(i);
        }
    }
    None
}

#[inline(never)]
fn find_free_scope(state: &mut MediatorState) -> Option<usize> {
    for i in 0..MAX_SCOPES {
        if !state.scopes[i].valid {
            return Some(i);
        }
    }
    None
}

#[inline(never)]
fn path_in_scope(pid: u16, request: &IoRequest, scopes: &[ScopeEntry; MAX_SCOPES]) -> bool {
    let Some(req_path) = request.path.as_ref().map(|path| &path[..request.path_len.min(64)]) else {
        return true;
    };

    if req_path == b"/proc/self" || req_path.starts_with(b"/proc/self/") {
        return true;
    }

    if let Some(scope) = scopes.iter().find(|entry| entry.valid && entry.pid == pid) {
        let scope_path = &scope.path[..scope.path_len.min(64)];
        return req_path.starts_with(scope_path);
    }

    false
}

fn accesses_other_proc(pid: u16, request: &IoRequest) -> bool {
    let Some(path_bytes) = request.path.as_ref().map(|path| &path[..request.path_len.min(64)]) else {
        return false;
    };
    let Ok(path) = core::str::from_utf8(path_bytes) else {
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

    if let Some(path) = request.path.as_ref() {
        for byte in &path[..request.path_len] {
            hash = hash.wrapping_mul(16_777_619) ^ u32::from(*byte);
        }
    }

    hash
}

fn scheduler_feedback(pid: u16, request: &IoRequest) {
    let penalty_intent_id = match request.kind {
        IoEventKind::NetworkSend => 101,
        IoEventKind::FileWrite => 102,
        _ => 0,
    };
    let now_ns = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct());
    crate::sched::queue::update_process_intent(pid, penalty_intent_id, now_ns);
}
