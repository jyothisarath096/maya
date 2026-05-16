use core::sync::atomic::Ordering;

pub const MAX_SNAP_PROCS: usize = 12;
pub const MAX_SNAP_FILES: usize = 40;

#[derive(Copy, Clone)]
pub struct ProcSnap {
    pub valid: bool,
    pub pid: u16,
    pub core_id: u8,
    pub intent: u8,
    pub name: [u8; 12],
    pub name_len: u8,
    pub cpu: u64,
    pub ipc_s: u64,
    pub ipc_r: u64,
    pub fires: u64,
    pub fw: u64,
    pub alm: u64,
    pub ack: u64,
    pub pkt: u64,
}

impl ProcSnap {
    pub const fn empty() -> Self {
        Self {
            valid: false,
            pid: 0,
            core_id: 0,
            intent: 0,
            name: [0; 12],
            name_len: 0,
            cpu: 0,
            ipc_s: 0,
            ipc_r: 0,
            fires: 0,
            fw: 0,
            alm: 0,
            ack: 0,
            pkt: 0,
        }
    }
}

#[derive(Copy, Clone)]
struct FsSnap {
    valid: bool,
    path: [u8; 48],
    path_len: u8,
    version_count: u32,
    active: bool,
}

impl FsSnap {
    const fn empty() -> Self {
        Self {
            valid: false,
            path: [0; 48],
            path_len: 0,
            version_count: 0,
            active: false,
        }
    }
}

pub static mut PROC_SNAP: [ProcSnap; MAX_SNAP_PROCS] = [ProcSnap::empty(); MAX_SNAP_PROCS];
pub static mut PROC_SNAP_COUNT: usize = 0;
static mut FS_SNAP: [FsSnap; MAX_SNAP_FILES] = [FsSnap::empty(); MAX_SNAP_FILES];
static mut FS_SNAP_COUNT: usize = 0;
static PREV_W_SUM: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);

fn uart_byte(b: u8) {
    crate::uart::write_byte(b);
}

fn uart_bytes(data: &[u8]) {
    for &b in data {
        uart_byte(b);
    }
}

fn uart_u64(mut v: u64) {
    if v == 0 {
        uart_byte(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut len = 0usize;
    while v > 0 {
        buf[len] = b'0' + (v % 10) as u8;
        v /= 10;
        len += 1;
    }
    buf[..len].reverse();
    uart_bytes(&buf[..len]);
}

fn uart_i32(v: i32) {
    if v < 0 {
        uart_byte(b'-');
        uart_u64(v.unsigned_abs() as u64);
    } else {
        uart_u64(v as u64);
    }
}

fn uart_json_str(data: &[u8], max_len: usize) {
    let mut written = 0usize;
    for &b in data {
        if written >= max_len {
            break;
        }
        match b {
            b'"' | b'\\' => {
                uart_byte(b'\\');
                uart_byte(b);
                written += 1;
            }
            0x20..=0x7e => {
                uart_byte(b);
                written += 1;
            }
            _ => {}
        }
    }
}

pub fn update_snapshot() {
    let mut proc_count = 0usize;
    if crate::proc::snapshot_for_telemetry(|snap| {
        if proc_count < MAX_SNAP_PROCS {
            unsafe {
                PROC_SNAP[proc_count] = snap;
            }
            proc_count += 1;
        }
    }) {
        unsafe {
            for slot in proc_count..MAX_SNAP_PROCS {
                PROC_SNAP[slot] = ProcSnap::empty();
            }
            PROC_SNAP_COUNT = proc_count;
        }
    }

    let mut fs_count = 0usize;
    if crate::fs::namespace::snapshot_entries_try(|path, file_id, is_dir| {
        if is_dir || fs_count >= MAX_SNAP_FILES {
            return;
        }
        if path.starts_with(b"/proc") {
            return;
        }
        if !(path.starts_with(b"/data/") || path.starts_with(b"/sys/")) {
            return;
        }
        let Some((version_count, active)) = crate::fs::store::file_metadata_try(file_id) else {
            return;
        };
        let mut snap = FsSnap::empty();
        snap.valid = true;
        let plen = path.len().min(snap.path.len());
        snap.path[..plen].copy_from_slice(&path[..plen]);
        snap.path_len = plen as u8;
        snap.version_count = version_count;
        snap.active = active;
        unsafe {
            FS_SNAP[fs_count] = snap;
        }
        fs_count += 1;
    }) {
        unsafe {
            for slot in fs_count..MAX_SNAP_FILES {
                FS_SNAP[slot] = FsSnap::empty();
            }
            FS_SNAP_COUNT = fs_count;
        }
    }
}

fn write_processes() {
    uart_bytes(b",\"p\":[");
    let count = unsafe { PROC_SNAP_COUNT };
    let mut first = true;
    for i in 0..count {
        let p = unsafe { &PROC_SNAP[i] };
        if !p.valid {
            continue;
        }
        if !first {
            uart_byte(b',');
        }
        first = false;
        uart_bytes(b"{\"id\":");
        uart_u64(p.pid as u64);
        uart_bytes(b",\"n\":\"");
        uart_json_str(&p.name[..p.name_len as usize], 12);
        uart_bytes(b"\",\"c\":");
        uart_u64(p.intent as u64);
        uart_bytes(b",\"k\":");
        uart_u64(p.core_id as u64);
        uart_bytes(b",\"s\":");
        uart_u64(p.cpu % 100);
        uart_bytes(b",\"ip\":");
        uart_u64(p.ipc_s);
        uart_bytes(b",\"ir\":");
        uart_u64(p.ipc_r);
        uart_bytes(b",\"al\":");
        uart_u64(p.alm);
        uart_bytes(b",\"ac\":");
        uart_u64(p.ack);
        uart_bytes(b",\"fw\":");
        uart_u64(p.fw);
        uart_bytes(b",\"pkt\":");
        uart_u64(p.pkt);
        uart_bytes(b",\"f\":");
        uart_u64(p.fires);
        uart_byte(b'}');
    }
    uart_byte(b']');
}

fn write_rewards() {
    let rewards = crate::sched::policy::get_last_rewards();
    uart_bytes(b",\"r\":[");
    for (i, reward) in rewards.iter().enumerate() {
        if i > 0 {
            uart_byte(b',');
        }
        uart_i32(*reward);
    }
    uart_byte(b']');

    let model = crate::model::weights::load();
    let w_sum: i32 = model.out_w.iter().map(|&w| w as i32).sum();
    let prev = PREV_W_SUM.load(Ordering::Relaxed);
    let delta = w_sum - prev;
    PREV_W_SUM.store(w_sum, Ordering::Relaxed);
    uart_bytes(b",\"w\":");
    uart_i32(w_sum);
    uart_bytes(b",\"d\":");
    uart_i32(delta);
}

fn write_fs_entries() {
    uart_bytes(b",\"fs\":[");
    let count = unsafe { FS_SNAP_COUNT };
    let mut first = true;
    for i in 0..count {
        let entry = unsafe { &FS_SNAP[i] };
        if !entry.valid {
            continue;
        }
        if !first {
            uart_byte(b',');
        }
        first = false;
        uart_bytes(b"{\"p\":\"");
        uart_json_str(&entry.path[..entry.path_len as usize], 48);
        uart_bytes(b"\",\"v\":");
        uart_u64(entry.version_count as u64);
        uart_bytes(b",\"a\":");
        uart_byte(if entry.active { b'1' } else { b'0' });
        uart_byte(b'}');
    }
    uart_byte(b']');
}

pub fn emit_frame() {
    if crate::uart::UART_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return;
    }

    uart_byte(0x02);
    uart_bytes(b"MAYA");
    uart_bytes(b"{\"t\":");
    uart_u64(crate::arch::timer::current_tick());
    write_processes();
    write_rewards();
    write_fs_entries();
    uart_byte(b'}');
    uart_byte(0x03);
    uart_byte(b'\n');

    crate::uart::UART_LOCK.store(false, Ordering::Release);
}
