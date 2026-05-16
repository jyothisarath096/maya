use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

const MAX_CORES: usize = 8;
static TICK_COUNTS: [AtomicU64; MAX_CORES] = [const { AtomicU64::new(0) }; MAX_CORES];
static RENDER_COUNTER: AtomicU32 = AtomicU32::new(0);
const TIMER_HZ: u64 = 100;

pub fn read_cntpct() -> u64 {
    let val: u64;
    unsafe {
        core::arch::asm!(
            "mrs {v}, cntpct_el0",
            v = out(reg) val,
            options(nomem, nostack, preserves_flags)
        );
    }
    val
}

pub fn cntfrq() -> u64 {
    let val: u64;
    unsafe {
        core::arch::asm!(
            "mrs {v}, cntfrq_el0",
            v = out(reg) val,
            options(nomem, nostack, preserves_flags)
        );
    }
    val
}

pub fn cntpct_to_ns(ticks: u64) -> u64 {
    let freq = cntfrq().max(1);
    let secs = ticks / freq;
    let rem = ticks % freq;
    secs.saturating_mul(1_000_000_000)
        .saturating_add(rem * 1_000_000_000 / freq)
}

pub fn init() {
    unsafe {
        let interval = cntfrq() / TIMER_HZ;
        core::arch::asm!(
            "msr cntv_tval_el0, {v}",
            v = in(reg) interval,
            options(nomem, nostack)
        );

        core::arch::asm!(
            "msr cntv_ctl_el0, {v}",
            v = in(reg) 1u64,
            options(nomem, nostack)
        );

        let isenabler0 = (0xFFFF_0000_0800_0000u64 + 0x100) as *mut u32;
        isenabler0.write_volatile(1 << 27);
    }
    crate::uart_print!("Timer initialised at 100Hz\n");
}

pub fn init_ap() {
    unsafe {
        core::arch::asm!(
            "msr cntp_ctl_el0, xzr",
            options(nomem, nostack)
        );
    }
}

pub fn enable_ap_timer() {
    let freq = cntfrq();
    // Give the AP enough time to reach EL0 before the first timer IRQ.
    let interval = freq;
    unsafe {
        core::arch::asm!(
            "msr cntp_tval_el0, {v}",
            "msr cntp_ctl_el0, {e}",
            v = in(reg) interval,
            e = in(reg) 1u64,
            options(nomem, nostack)
        );
    }
}

pub fn handle_tick() {
    let core_id = crate::arch::cpu::current_core_id() as usize;
    TICK_COUNTS[core_id].fetch_add(1, Ordering::Relaxed);
    if core_id == 0 {
        crate::input::keyboard::poll();
        let rc = RENDER_COUNTER.fetch_add(1, Ordering::Relaxed);
        if rc % 50 == 0 {
            crate::telemetry::update_snapshot();
            crate::telemetry::emit_frame();
            crate::gpu::canvas::render_frame();
        }
    }

    let interval = cntfrq() / TIMER_HZ;
    unsafe {
        if core_id == 0 {
            core::arch::asm!(
                "msr cntv_tval_el0, {v}",
                v = in(reg) interval,
                options(nomem, nostack)
            );
        } else {
            core::arch::asm!(
                "msr cntp_tval_el0, {v}",
                v = in(reg) interval,
                options(nomem, nostack)
            );
        }
    }
    crate::sched::queue::tick_handler();
}

pub fn current_tick() -> u64 {
    TICK_COUNTS[0].load(Ordering::Relaxed)
}
