#![allow(dead_code)]

use crate::{
    fb_print,
    io::{audit::IoEventKind, mediator, syscall::IoRequest},
    serial_print,
};

#[repr(C)]
struct PerCpuData {
    kernel_rsp: u64,
    user_rsp: u64,
}

static mut PER_CPU: PerCpuData = PerCpuData {
    kernel_rsp: 0,
    user_rsp: 0,
};

static mut SYSCALL_STACK: [u8; 65536] = [0u8; 65536];

pub const SYS_EXIT: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_READ: u64 = 2;
pub const SYS_SPAWN: u64 = 3;
pub const SYS_YIELD: u64 = 4;
pub const SYS_GETCAP: u64 = 5;
pub const SYS_QUERY: u64 = 6;
pub const SYS_INTENT: u64 = 7;
pub const SYS_TELEMETRY: u64 = 8;
pub const SYS_TELEMETRY_ALIAS: u64 = 0x88;

pub const CAP_STDOUT: u64 = 0;
pub const CAP_STDIN: u64 = 1;
pub const CAP_STDERR: u64 = 2;

pub fn init() {
    unsafe {
        let stack_top = core::ptr::addr_of!(SYSCALL_STACK) as u64 + 65536;
        PER_CPU.kernel_rsp = stack_top;

        let per_cpu_addr = core::ptr::addr_of!(PER_CPU) as u64;
        wrmsr(0xC000_0101, per_cpu_addr);
        wrmsr(0xC000_0102, per_cpu_addr);

        wrmsr(0xC000_0081, (0x08u64 << 32) | (0x18u64 << 48));
        wrmsr(0xC000_0082, syscall_entry as *const () as u64);
        wrmsr(0xC000_0084, 1 << 9);
        let efer = rdmsr(0xC000_0080);
        wrmsr(0xC000_0080, efer | 1);
    }
    serial_print("STAR MSR set\n");
    serial_print("Syscall interface initialised\n");
    fb_print("Syscall interface initialised\n");
}

#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        "swapgs",
        "mov gs:8, rsp",
        "mov rsp, gs:0",
        "push rcx",
        "push r11",
        "push rax",
        "push rdi",
        "push rsi",
        "push rdx",
        "push r10",
        "push r8",
        "push r9",
        "mov rcx, rdx",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "mov rdi, rax",
        "call {dispatch}",
        "pop r9",
        "pop r8",
        "pop r10",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rax",
        "pop r11",
        "pop rcx",
        "mov rsp, gs:8",
        "swapgs",
        "sysretq",
        dispatch = sym syscall_dispatch,
    );
}

extern "C" fn syscall_dispatch(nr: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    match nr {
        SYS_EXIT => sys_exit(arg1 as i32),
        SYS_WRITE => sys_write(arg1, arg2 as *const u8, arg3 as usize),
        SYS_READ => sys_read(arg1, arg2 as *mut u8, arg3 as usize),
        SYS_GETCAP => sys_getcap(arg1),
        SYS_YIELD => {
            sys_yield();
            0
        }
        SYS_QUERY => sys_query(arg1 as *const u8, arg2 as usize),
        SYS_INTENT => sys_intent(arg1 as *const u8, arg2 as usize),
        SYS_TELEMETRY | SYS_TELEMETRY_ALIAS => sys_telemetry(arg1, arg2, arg3),
        _ => -1,
    }
}

fn sys_exit(code: i32) -> ! {
    let _ = code;
    unsafe {
        let kr = crate::memory::vmm::kernel_cr3();
        core::arch::asm!(
            "mov cr3, {cr3}",
            cr3 = in(reg) kr,
            options(nomem, nostack, preserves_flags)
        );
    }
    serial_print("Process exited\n");
    fb_print("Process exited\n");
    crate::proc::unmap_current_process();
    crate::proc::mark_process_done();
    unsafe {
        core::arch::asm!(
            "int3",
            options(nomem, nostack)
        );
    }
    loop {
        unsafe {
            core::arch::asm!(
                "hlt",
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

fn sys_write(_cap: u64, buf: *const u8, len: usize) -> i64 {
    if len == 0 {
        return 0;
    }
    if len > 4096 {
        return -1;
    }

    let request = IoRequest {
        kind: IoEventKind::FileWrite,
        path: None,
        size: len,
        offset: 0,
    };
    let decision = mediator::mediate(1, &request);
    if matches!(decision.decision, crate::io::audit::MediatorDecision::Block) {
        return -1;
    }

    unsafe {
        let slice = core::slice::from_raw_parts(buf, len);
        if let Ok(s) = core::str::from_utf8(slice) {
            serial_print(s);
            fb_print(s);
        }
    }
    len as i64
}

fn sys_read(_cap: u64, _buf: *mut u8, _len: usize) -> i64 {
    0
}

fn sys_getcap(id: u64) -> i64 {
    match id {
        CAP_STDOUT => 1,
        CAP_STDIN => 2,
        CAP_STDERR => 3,
        _ => -1,
    }
}

fn sys_yield() {
}

fn sys_query(_question: *const u8, _len: usize) -> i64 {
    0
}

fn sys_intent(intent_ptr: *const u8, len: usize) -> i64 {
    if len == 0 || len > 64 {
        return -1;
    }

    unsafe {
        let kr = crate::memory::vmm::kernel_cr3();
        core::arch::asm!(
            "mov cr3, {cr3}",
            cr3 = in(reg) kr,
            options(nomem, nostack, preserves_flags)
        );
    }

    unsafe {
        let slice = core::slice::from_raw_parts(intent_ptr, len);
        if let Ok(name) = core::str::from_utf8(slice) {
            let pid = crate::sched::queue::current_pid().unwrap_or(1);
            crate::proc::register_intent(pid, name);
            serial_print("INTENT registered: ");
            serial_print(name);
            serial_print("\n");
            static mut INTENT_COUNTER: u64 = 100;
            INTENT_COUNTER += 1;
            INTENT_COUNTER as i64
        } else {
            -1
        }
    }
}

fn sys_telemetry(intent_id: u64, _data_ptr: u64, _arg3: u64) -> i64 {
    unsafe {
        let kr = crate::memory::vmm::kernel_cr3();
        core::arch::asm!(
            "mov cr3, {cr3}",
            cr3 = in(reg) kr,
            options(nomem, nostack, preserves_flags)
        );
    }

    for &byte in b"T:" {
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") 0x3F8u16,
                in("al") byte,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
    let hundreds = (intent_id / 100) as u8;
    let tens = ((intent_id % 100) / 10) as u8;
    let ones = (intent_id % 10) as u8;
    for digit in [hundreds, tens, ones] {
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") 0x3F8u16,
                in("al") (b'0' + digit),
                options(nomem, nostack, preserves_flags)
            );
        }
    }
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") 0x3F8u16,
            in("al") b'\n',
            options(nomem, nostack, preserves_flags)
        );
    }
    let current_tick = crate::sched::timer::current_tick();
    let pid = crate::proc::get_current_pid();
    crate::sched::queue::update_process_intent(pid, intent_id as u16, current_tick);
    0
}

unsafe fn wrmsr(msr: u32, val: u64) {
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") val as u32,
        in("edx") (val >> 32) as u32,
        options(nomem, nostack, preserves_flags)
    );
}

unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") lo,
        out("edx") hi,
        options(nomem, nostack, preserves_flags)
    );
    ((hi as u64) << 32) | lo as u64
}
