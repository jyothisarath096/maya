#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ProcessClass {
    Idle = 0,
    Batch = 1,
    Interactive = 2,
    Realtime = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ProcessState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

#[derive(Debug, Clone)]
pub struct ProcessStats {
    pub cpu_ticks_used: u64,
    pub ticks_waiting: u64,
    pub io_wait_ticks: u64,
    pub ipc_sends: u64,
    pub ipc_recvs: u64,
    pub page_faults: u64,
    pub last_scheduled: u64,
    pub last_input_tick: u64,
    pub burst_ticks: u64,
    pub total_ticks_alive: u64,
    pub last_intent_id: u16,
    pub intent_fire_count: u64,
    pub intent_fire_tick: u64,
}

pub const INTENT_NET_SEND: u16 = 101;
pub const INTENT_FILE_SAVE: u16 = 102;
pub const INTENT_CRYPTO: u16 = 103;
pub const INTENT_UI_RENDER: u16 = 104;
pub const INTENT_DB_WRITE: u16 = 105;

pub fn intent_weight(intent_id: u16) -> f32 {
    match intent_id {
        0 => 0.0,
        1..=10 => 0.1,
        100 => 0.3,
        INTENT_NET_SEND => 0.9,
        INTENT_FILE_SAVE => 0.8,
        INTENT_CRYPTO => 1.0,
        INTENT_UI_RENDER => 0.7,
        INTENT_DB_WRITE => 0.85,
        _ => 0.5,
    }
}

#[derive(Debug, Clone)]
pub struct Process {
    pub pid: u16,
    pub class: ProcessClass,
    pub state: ProcessState,
    pub priority: f32,
    pub priority_hint: f32,
    pub deadline_tick: Option<u64>,
    pub stats: ProcessStats,
    pub cap_count: u16,
    pub memory_pages: u32,
    pub ipc_partner: Option<u16>,
}

impl Process {
    pub fn new(pid: u16, class: ProcessClass, priority_hint: f32) -> Self {
        Self {
            pid,
            class,
            state: ProcessState::Ready,
            priority: 0.0,
            priority_hint: clamp01(priority_hint),
            deadline_tick: None,
            stats: ProcessStats {
                cpu_ticks_used: 0,
                ticks_waiting: 0,
                io_wait_ticks: 0,
                ipc_sends: 0,
                ipc_recvs: 0,
                page_faults: 0,
                last_scheduled: 0,
                last_input_tick: 0,
                burst_ticks: 0,
                total_ticks_alive: 0,
                last_intent_id: 0,
                intent_fire_count: 0,
                intent_fire_tick: 0,
            },
            cap_count: 0,
            memory_pages: 0,
            ipc_partner: None,
        }
    }

    pub fn to_features(&self, current_tick: u64, max_pages: u32, max_caps: u16) -> [f32; 16] {
        let alive = self.stats.total_ticks_alive.max(1);
        let tick_norm = current_tick.max(1);
        let class_norm = (self.class as u8) as f32 / 3.0;
        let cpu_usage_pct = ratio_u64(self.stats.cpu_ticks_used, alive);
        let wait_time_ticks = ratio_u64(
            current_tick.saturating_sub(self.stats.last_scheduled),
            tick_norm,
        );
        let io_wait_ratio = ratio_u64(self.stats.io_wait_ticks, alive);
        let ipc_send_rate = ratio_u64(self.stats.ipc_sends, alive);
        let ipc_recv_rate = ratio_u64(self.stats.ipc_recvs, alive);
        let memory_pages = ratio_u32(self.memory_pages, max_pages.max(1));
        let page_fault_rate = ratio_u64(self.stats.page_faults, alive);
        let priority_hint = clamp01(self.priority_hint);
        let time_since_input = ratio_u64(
            current_tick.saturating_sub(self.stats.last_input_tick),
            tick_norm,
        );
        let burst_length = ratio_u64(self.stats.burst_ticks, alive);
        let deadline_urgency = match self.deadline_tick {
            None => 0.0,
            Some(deadline) if deadline <= current_tick => 1.0,
            Some(deadline) => {
                let remaining = deadline.saturating_sub(current_tick);
                let window = deadline.max(1);
                clamp01(1.0 - (remaining as f32 / window as f32))
            }
        };
        let intent_w = intent_weight(self.stats.last_intent_id);
        let starvation_risk = ratio_u64(self.stats.ticks_waiting, tick_norm);
        let intent_recency = if self.stats.intent_fire_tick > 0
            && current_tick > self.stats.intent_fire_tick
        {
            let age = current_tick - self.stats.intent_fire_tick;
            if age < 100 {
                1.0 - (age as f32 / 100.0)
            } else {
                0.0
            }
        } else {
            0.0
        };
        let cap_count = ratio_u16(self.cap_count, max_caps.max(1));

        [
            class_norm,
            cpu_usage_pct,
            wait_time_ticks,
            io_wait_ratio,
            ipc_send_rate,
            ipc_recv_rate,
            memory_pages,
            page_fault_rate,
            priority_hint,
            time_since_input,
            burst_length,
            deadline_urgency,
            intent_w,
            starvation_risk,
            intent_recency,
            cap_count,
        ]
    }
}

fn ratio_u64(value: u64, max: u64) -> f32 {
    clamp01(value as f32 / max as f32)
}

fn ratio_u32(value: u32, max: u32) -> f32 {
    clamp01(value as f32 / max as f32)
}

fn ratio_u16(value: u16, max: u16) -> f32 {
    clamp01(value as f32 / max as f32)
}

fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}
