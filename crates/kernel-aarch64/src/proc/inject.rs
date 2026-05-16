#![allow(dead_code)]

use crate::{
    arch::exceptions::SyscallFrame,
    cap::{self, Rights},
    proc,
    KernelError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InjectionStatus {
    Pending = 0,
    Complete = 1,
    Timeout = 2,
    CapDenied = 3,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct InjectionResult {
    pub intent_id: u16,
    pub return_lo: u64,
    pub return_hi: u64,
    pub latency_ns: u64,
    pub status: InjectionStatus,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct InjectionState {
    pub pending: bool,
    pub intent_id: u16,
    pub saved_elr: u64,
    pub saved_sp_el0: u64,
    pub saved_x0_x7: [u64; 8],
    pub result: InjectionResult,
    pub inject_time_ns: u64,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct InjectionRequest {
    pub target_pid: u16,
    pub intent_id: u16,
    pub args: [u64; 8],
    pub timeout_ns: u64,
}

pub fn inject_function_call(
    frame: &mut SyscallFrame,
    req: InjectionRequest,
) -> Result<(), KernelError> {
    let func_vaddr = resolve_injection_target(req)?;
    unsafe {
        let dr = 0xFFFF_0000_0900_0000u64 as *mut u32;
        for &b in b"INJ_VADDR=" {
            dr.write_volatile(b as u32);
        }
        for i in (0..8).rev() {
            let nibble = ((func_vaddr >> (i * 4)) & 0xF) as u8;
            let c = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
            dr.write_volatile(c as u32);
        }
        dr.write_volatile(b'\n' as u32);
    }
    let inject_return_vaddr = proc::get_inject_return_vaddr(req.target_pid)
        .ok_or(KernelError::ProcNotFound)?;

    let saved_elr = frame.elr;
    let inject_time_ns =
        crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct());
    proc::set_injection_state(
        req.target_pid,
        InjectionState {
            pending: true,
            intent_id: req.intent_id,
            saved_elr,
            saved_sp_el0: frame.sp_el0,
            saved_x0_x7: frame.x[..8].try_into().unwrap(),
            result: InjectionResult {
                intent_id: req.intent_id,
                return_lo: 0,
                return_hi: 0,
                latency_ns: 0,
                status: InjectionStatus::Pending,
            },
            inject_time_ns,
        },
    );

    for (index, arg) in req.args.iter().enumerate() {
        frame.x[index] = *arg;
    }
    frame.x[2] = req.intent_id as u64;
    frame.x[30] = inject_return_vaddr;
    frame.elr = func_vaddr;

    Ok(())
}

pub fn inject_context_call(
    frame: &mut proc::ContextFrame,
    req: InjectionRequest,
) -> Result<(), KernelError> {
    let func_vaddr = resolve_injection_target(req)?;
    unsafe {
        let dr = 0xFFFF_0000_0900_0000u64 as *mut u32;
        for &b in b"INJ_VADDR=" {
            dr.write_volatile(b as u32);
        }
        for i in (0..8).rev() {
            let nibble = ((func_vaddr >> (i * 4)) & 0xF) as u8;
            let c = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
            dr.write_volatile(c as u32);
        }
        dr.write_volatile(b'\n' as u32);
    }
    let inject_return_vaddr = proc::get_inject_return_vaddr(req.target_pid)
        .ok_or(KernelError::ProcNotFound)?;

    proc::set_injection_state(
        req.target_pid,
        InjectionState {
            pending: true,
            intent_id: req.intent_id,
            saved_elr: frame.elr,
            saved_sp_el0: frame.sp_el0,
            saved_x0_x7: frame.x[..8].try_into().unwrap(),
            result: InjectionResult {
                intent_id: req.intent_id,
                return_lo: 0,
                return_hi: 0,
                latency_ns: 0,
                status: InjectionStatus::Pending,
            },
            inject_time_ns: crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct()),
        },
    );

    for (index, arg) in req.args.iter().enumerate() {
        frame.x[index] = *arg;
    }
    frame.x[2] = req.intent_id as u64;
    frame.x[30] = inject_return_vaddr;
    frame.elr = func_vaddr;
    Ok(())
}

fn resolve_injection_target(req: InjectionRequest) -> Result<u64, KernelError> {
    let token = proc::get_process_intent_cap(req.target_pid, req.intent_id)
        .ok_or(KernelError::CapInvalidToken)?;
    cap::check_right(token, Rights::INTENT_CALL)?;
    proc::get_intent_vaddr(req.target_pid, req.intent_id)
        .ok_or(KernelError::ProcNotFound)
}

pub fn complete_injection(
    frame: &mut SyscallFrame,
    intent_id: u16,
    return_lo: u64,
    return_hi: u64,
) {
    let core_id = crate::arch::cpu::current_core_id() as usize;
    let pid = proc::current_process_for_core(core_id);
    let Some(state) = proc::get_injection_state(pid) else {
        return;
    };
    if !state.pending || state.intent_id != intent_id {
        return;
    }

    let now_ns = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct());
    let latency_ns = now_ns.saturating_sub(state.inject_time_ns);

    frame.elr = state.saved_elr;
    frame.sp_el0 = state.saved_sp_el0;
    unsafe {
        core::arch::asm!(
            "msr sp_el0, {sp}",
            "isb",
            sp = in(reg) state.saved_sp_el0,
            options(nomem, nostack, preserves_flags)
        );
    }
    for (index, value) in state.saved_x0_x7.iter().enumerate() {
        frame.x[index] = *value;
    }

    proc::store_injection_result(
        pid,
        InjectionResult {
            intent_id,
            return_lo,
            return_hi,
            latency_ns,
            status: InjectionStatus::Complete,
        },
    );
    proc::clear_injection_state(pid);
}
