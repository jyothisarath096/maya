use core::arch::global_asm;
use core::sync::atomic::{AtomicBool, Ordering};

global_asm!(include_str!("vectors.s"));
global_asm!(include_str!("context.s"));

#[repr(align(2048))]
pub struct ExceptionVectors {
    _data: [u8; 2048],
}

#[repr(C)]
pub struct SyscallFrame {
    pub x: [u64; 31],
    pub elr: u64,
    pub spsr: u64,
    pub sp_el0: u64,
}

pub fn init() {
    unsafe extern "C" {
        static exception_vectors: u8;
    }
    unsafe {
        let vbar = core::ptr::addr_of!(exception_vectors) as u64;
        core::arch::asm!(
            "msr vbar_el1, {vbar}",
            "isb",
            vbar = in(reg) vbar,
            options(nomem, nostack)
        );
    }
    crate::uart_print!("Exception vectors installed\n");
}

#[unsafe(no_mangle)]
pub extern "C" fn sync_handler_el1() {
    let elr: u64;
    let esr: u64;
    let far: u64;
    let sp: u64;
    unsafe {
        core::arch::asm!("mrs {}, elr_el1", out(reg) elr, options(nomem, nostack));
        core::arch::asm!("mrs {}, esr_el1", out(reg) esr, options(nomem, nostack));
        core::arch::asm!("mrs {}, far_el1", out(reg) far, options(nomem, nostack));
        core::arch::asm!("mov {}, sp", out(reg) sp, options(nomem, nostack));
    }
    crate::uart_print!("EXCEPTION: SYNC EL1\n");
    crate::uart_print!("  ELR: ");
    crate::uart_print_hex!(elr);
    crate::uart_print!("\n  ESR: ");
    crate::uart_print_hex!(esr);
    crate::uart_print!("\n  FAR: ");
    crate::uart_print_hex!(far);
    crate::uart_print!("\n  SP:  ");
    crate::uart_print_hex!(sp);
    crate::uart_print!("\n");
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn svc_handler_el0(frame: *mut SyscallFrame) {
    static PAN_CHECKED: AtomicBool = AtomicBool::new(false);
    if !PAN_CHECKED.load(Ordering::Relaxed) {
        PAN_CHECKED.store(true, Ordering::Relaxed);
        let pan: u64;
        unsafe {
            core::arch::asm!(
                "mrs {v}, pan",
                v = out(reg) pan,
                options(nomem, nostack)
            );
        }
        if pan != 0 {
            crate::uart_print!("PAN: active in SVC\n");
        } else {
            crate::uart_print!("PAN: NOT active in SVC\n");
        }
    }
    let frame_ref = unsafe { &mut *frame };
    let nr = frame_ref.x[8];
    if nr == 0x01 {
        crate::proc::syscall::sys_yield(frame_ref);
        return;
    }
    if nr == 0x8A {
        let result = crate::proc::syscall::dispatch_injection(
            frame_ref,
            frame_ref.x[0],
            frame_ref.x[1],
            frame_ref.x[2],
            frame_ref.x[3],
        );
        frame_ref.x[0] = result.0 as u64;
        frame_ref.x[1] = result.1 as u64;
        return;
    }
    let a0 = frame_ref.x[0];
    let a1 = frame_ref.x[1];
    let a2 = frame_ref.x[2];
    let a3 = frame_ref.x[3];

    let result = if nr & 0xFF00 == 0x0100 {
        crate::proc::syscall::dispatch_io(nr, a0, a1, a2, a3)
    } else if nr == 0x80 || nr == 0x89 || (nr & 0xFF) == 0x88 {
        let (intent_nr, intent_a0) = if (nr & 0xFF) == 0x88 && nr != 0x88 {
            (0x88, nr >> 8)
        } else {
            (nr, a0)
        };
        crate::proc::syscall::dispatch_intent(frame_ref, intent_nr, intent_a0, a1, a2, a3)
    } else {
        match nr {
            0x00..=0x0F => crate::proc::syscall::dispatch_core(nr, a0, a1, a2, a3),
            0x10..=0x1F => crate::proc::syscall::dispatch_cap(nr, a0, a1, a2, a3),
            0x20..=0x2F => crate::proc::syscall::dispatch_ipc(nr, a0, a1, a2, a3),
            0x30..=0x3F => crate::proc::syscall::dispatch_fs(nr, a0, a1, a2, a3),
            0x40..=0x4F => crate::proc::syscall::dispatch_net(nr, a0, a1, a2, a3),
            0x50..=0x5F => crate::proc::syscall::dispatch_input(nr, a0, a1, a2, a3),
            _ => (-1, 0),
        }
    };

    frame_ref.x[0] = result.0 as u64;
    frame_ref.x[1] = result.1 as u64;
}

#[unsafe(no_mangle)]
pub extern "C" fn fault_handler_el0() {
    let elr: u64;
    let esr: u64;
    let far: u64;
    let sp_el0: u64;
    unsafe {
        let dr = 0xFFFF_0000_0900_0000u64 as *mut u32;
        for &b in b"EL0 FAULT\r\n" {
            dr.write_volatile(b as u32);
        }
        core::arch::asm!("mrs {}, elr_el1", out(reg) elr, options(nomem, nostack));
        core::arch::asm!("mrs {}, esr_el1", out(reg) esr, options(nomem, nostack));
        core::arch::asm!("mrs {}, far_el1", out(reg) far, options(nomem, nostack));
        core::arch::asm!("mrs {}, sp_el0", out(reg) sp_el0, options(nomem, nostack));
    }
    crate::uart_print!("EXCEPTION: EL0 SYNC FAULT\n");
    crate::uart_print!("  ELR: ");
    crate::uart_print_hex!(elr);
    crate::uart_print!("\n  ESR: ");
    crate::uart_print_hex!(esr);
    crate::uart_print!("\n  FAR: ");
    crate::uart_print_hex!(far);
    crate::uart_print!("\n  SP_EL0: ");
    crate::uart_print_hex!(sp_el0);
    crate::uart_print!("\n");

    let frame_ptr: u64;
    unsafe {
        core::arch::asm!("mrs {}, tpidr_el1", out(reg) frame_ptr, options(nomem, nostack));
    }
    if frame_ptr != 0 {
        let x29 = unsafe { *((frame_ptr + 232) as *const u64) };
        let x30 = unsafe { *((frame_ptr + 240) as *const u64) };
        let saved_sp = unsafe { *((frame_ptr + 264) as *const u64) };
        crate::uart_print!("  saved x29: ");
        crate::uart_print_hex!(x29);
        crate::uart_print!("\n  saved x30: ");
        crate::uart_print_hex!(x30);
        crate::uart_print!("\n  saved sp: ");
        crate::uart_print_hex!(saved_sp);
        crate::uart_print!("\n");
    }

    let core_id = crate::arch::cpu::current_core_id() as usize;
    let pid = crate::proc::current_process_for_core(core_id);
    crate::uart_print!("  PID: ");
    crate::uart_print_usize!(pid as usize);
    crate::uart_print!(" terminated\n");

    crate::sched::queue::remove_process(pid);
    crate::proc::set_current_process_for_core(core_id, 0);
    crate::proc::set_current_pid(0);

    unsafe {
        core::arch::asm!(
            "msr daifclr, #2",
            "1: wfe",
            "b 1b",
            options(noreturn)
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn irq_handler_el1() {
    crate::arch::gic::handle_irq();
}

#[unsafe(no_mangle)]
pub extern "C" fn irq_context_switch_handler(
    frame: *mut crate::proc::ContextFrame,
) -> *mut crate::proc::ContextFrame {
    let ack = crate::arch::gic::acknowledge_irq();
    let irq_id = ack & 0x3FF;

    if irq_id == 1023 {
        return frame;
    }

    if irq_id == 27 {
        crate::arch::timer::handle_tick();
    } else {
        crate::arch::gic::handle_irq_id(irq_id);
    }
    crate::arch::gic::end_irq(ack);

    let core_id = crate::arch::cpu::current_core_id() as usize;
    if !crate::sched::queue::core_has_work(core_id) {
        return frame;
    }
    let current_pid = crate::proc::current_process_for_core(core_id);
    let current_pid_opt = if current_pid == 0 {
        None
    } else {
        Some(current_pid)
    };
    let next_pid = crate::sched::queue::choose_next_process(current_pid_opt).unwrap_or(0);

    if next_pid == 0 || next_pid == current_pid {
        return frame;
    }

    if current_pid != 0 {
        crate::proc::save_process_frame(current_pid, frame);
    }

    let Some((ttbr0, asid)) = crate::proc::get_process_ttbr0(next_pid) else {
        return frame;
    };
    crate::memory::vmm::set_user_table(ttbr0, asid);
    crate::proc::set_current_process_for_core(core_id, next_pid);
    crate::proc::set_current_pid(next_pid);

    let next_frame = crate::proc::get_process_frame(next_pid);
    if next_frame.is_null() {
        return frame;
    }

    unsafe {
        core::arch::asm!(
            "msr tpidr_el0, {pid}",
            "msr tpidr_el1, {frame}",
            "isb",
            pid = in(reg) next_pid as u64,
            frame = in(reg) next_frame as u64,
            options(nomem, nostack, preserves_flags)
        );
    }

    next_frame
}

#[unsafe(no_mangle)]
pub extern "C" fn fiq_handler_el1() {
    crate::uart_print!("EXCEPTION: FIQ\n");
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn serror_handler_el1() {
    unsafe {
        let dr = 0xFFFF_0000_0900_0000u64 as *mut u32;
        for &b in b"SERROR HIT\r\n" {
            dr.write_volatile(b as u32);
        }
    }
    let elr: u64;
    let esr: u64;
    unsafe {
        core::arch::asm!("mrs {}, elr_el1", out(reg) elr, options(nomem, nostack));
        core::arch::asm!("mrs {}, esr_el1", out(reg) esr, options(nomem, nostack));
    }
    if elr < 0x4000_0000 {
        crate::uart_print!("SError from EL0 at ");
        crate::uart_print_hex!(elr);
        crate::uart_print!(" - resuming\n");
        unsafe {
            core::arch::asm!(
                "msr daifset, #4",
                "isb",
                "eret",
                options(noreturn)
            );
        }
    }
    let far: u64;
    let sp: u64;
    let ttbr0: u64;
    let lr: u64;
    unsafe {
        core::arch::asm!("mrs {}, far_el1", out(reg) far, options(nomem, nostack));
        core::arch::asm!("mov {}, sp", out(reg) sp, options(nomem, nostack));
        core::arch::asm!("mrs {}, ttbr0_el1", out(reg) ttbr0, options(nomem, nostack));
        core::arch::asm!("mov {}, x30", out(reg) lr, options(nomem, nostack));
    }
    crate::uart_print!("EXCEPTION: SERROR\n");
    crate::uart_print!("  ELR:   ");
    crate::uart_print_hex!(elr);
    crate::uart_print!("\n  ESR:   ");
    crate::uart_print_hex!(esr);
    crate::uart_print!("\n  FAR:   ");
    crate::uart_print_hex!(far);
    crate::uart_print!("\n  SP:    ");
    crate::uart_print_hex!(sp);
    crate::uart_print!("\n  TTBR0: ");
    crate::uart_print_hex!(ttbr0);
    crate::uart_print!("\n  LR:    ");
    crate::uart_print_hex!(lr);
    crate::uart_print!("\n");
    loop {}
}
