#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::{AtomicUsize, Ordering},
};

pub mod arch;
pub mod cap;
pub mod debug;
pub mod fs;
pub mod gpu;
pub mod input;
pub mod io;
pub mod ipc;
pub mod model;
pub mod memory;
pub mod net;
pub mod proc;
mod rt_mem;
pub mod sched;
pub mod telemetry;
pub mod uart;

struct BumpAllocator;

const HEAP_SIZE: usize = 1024 * 1024;

#[repr(align(16))]
struct Heap([u8; HEAP_SIZE]);

static HEAP_NEXT: AtomicUsize = AtomicUsize::new(0);
static mut HEAP: Heap = Heap([0; HEAP_SIZE]);

const COMPUTE_MEXE: &[u8] =
    include_bytes!("../../../userspace/compute_workload/compute_workload.mexe");
const COMPUTE_MSHM: &[u8] =
    include_bytes!("../../../userspace/compute_workload/compute_workload.mshm");
const COMPUTE_MLMB: &[u8] =
    include_bytes!("../../../userspace/compute_workload/compute_workload.mlmb");
const IO_MEXE: &[u8] = include_bytes!("../../../userspace/io_workload/io_workload.mexe");
const IO_MSHM: &[u8] = include_bytes!("../../../userspace/io_workload/io_workload.mshm");
const IO_MLMB: &[u8] = include_bytes!("../../../userspace/io_workload/io_workload.mlmb");
const BG_MEXE: &[u8] =
    include_bytes!("../../../userspace/background_task/background_task.mexe");
const BG_MSHM: &[u8] =
    include_bytes!("../../../userspace/background_task/background_task.mshm");
const BG_MLMB: &[u8] =
    include_bytes!("../../../userspace/background_task/background_task.mlmb");
const MATRIX_MEXE: &[u8] =
    include_bytes!("../../../userspace/matrix_multiply/matrix_multiply.mexe");
const MATRIX_MSHM: &[u8] =
    include_bytes!("../../../userspace/matrix_multiply/matrix_multiply.mshm");
const MATRIX_MLMB: &[u8] =
    include_bytes!("../../../userspace/matrix_multiply/matrix_multiply.mlmb");
const NET_MEXE: &[u8] = include_bytes!("../../../userspace/net_parser/net_parser.mexe");
const NET_MSHM: &[u8] = include_bytes!("../../../userspace/net_parser/net_parser.mshm");
const NET_MLMB: &[u8] = include_bytes!("../../../userspace/net_parser/net_parser.mlmb");
const SORT_MEXE: &[u8] = include_bytes!("../../../userspace/sort_suite/sort_suite.mexe");
const SORT_MSHM: &[u8] = include_bytes!("../../../userspace/sort_suite/sort_suite.mshm");
const SORT_MLMB: &[u8] = include_bytes!("../../../userspace/sort_suite/sort_suite.mlmb");
const MRT_HELLO_MEXE: &[u8] = include_bytes!("../../../userspace/mrt_hello/mrt_hello.mexe");
const MRT_HELLO_MSHM: &[u8] = include_bytes!("../../../userspace/mrt_hello/mrt_hello.mshm");
const MRT_HELLO_MLMB: &[u8] = include_bytes!("../../../userspace/mrt_hello/mrt_hello.mlmb");
const MRT_PRODUCER_MEXE: &[u8] =
    include_bytes!("../../../userspace/mrt_producer/mrt_producer.mexe");
const MRT_PRODUCER_MSHM: &[u8] =
    include_bytes!("../../../userspace/mrt_producer/mrt_producer.mshm");
const MRT_PRODUCER_MLMB: &[u8] =
    include_bytes!("../../../userspace/mrt_producer/mrt_producer.mlmb");
const MRT_CONSUMER_MEXE: &[u8] =
    include_bytes!("../../../userspace/mrt_consumer/mrt_consumer.mexe");
const MRT_CONSUMER_MSHM: &[u8] =
    include_bytes!("../../../userspace/mrt_consumer/mrt_consumer.mshm");
const MRT_CONSUMER_MLMB: &[u8] =
    include_bytes!("../../../userspace/mrt_consumer/mrt_consumer.mlmb");
const MRT_LOGGER_MEXE: &[u8] = include_bytes!("../../../userspace/mrt_logger/mrt_logger.mexe");
const MRT_LOGGER_MSHM: &[u8] = include_bytes!("../../../userspace/mrt_logger/mrt_logger.mshm");
const MRT_LOGGER_MLMB: &[u8] = include_bytes!("../../../userspace/mrt_logger/mrt_logger.mlmb");
const MRT_SHELL_MEXE: &[u8] = include_bytes!("../../../userspace/mrt_shell/mrt_shell.mexe");
const MRT_SHELL_MSHM: &[u8] = include_bytes!("../../../userspace/mrt_shell/mrt_shell.mshm");
const MRT_SHELL_MLMB: &[u8] = include_bytes!("../../../userspace/mrt_shell/mrt_shell.mlmb");

#[global_allocator]
static GLOBAL_ALLOCATOR: BumpAllocator = BumpAllocator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelError {
    CapInvalidToken,
    CapTableFull,
    CapInsufficientRights,
    CapDelegationDepthExceeded,
    CapMTEFault,
    CapPACFailed,
    IpcNotInitialized,
    IpcInvalidChannel,
    IpcChannelFull,
    IpcChannelEmpty,
    IpcChannelClosed,
    IpcTimeout,
    IpcNoRoute,
    IpcCapTransferDenied,
    ProcTableFull,
    ProcNotFound,
    InvalidElf,
    ElfLoadFailed,
    VmmMapFailed,
    AsidinvalId,
    InvalidArgument,
    OutOfMemory,
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(1);
        let size = layout.size();
        let base = core::ptr::addr_of_mut!(HEAP.0) as *mut u8 as usize;

        let result = HEAP_NEXT.fetch_update(Ordering::AcqRel, Ordering::Acquire, |offset| {
            let aligned = (offset + (align - 1)) & !(align - 1);
            let next = aligned.checked_add(size)?;
            if next > HEAP_SIZE {
                None
            } else {
                Some(next)
            }
        });

        match result {
            Ok(offset) => {
                let aligned = (offset + (align - 1)) & !(align - 1);
                (base + aligned) as *mut u8
            }
            Err(_) => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    crate::uart_print!("ALLOC ERROR\n");
    loop {
        unsafe {
            core::arch::asm!("wfe", options(nomem, nostack, preserves_flags));
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    crate::uart::init();
    crate::uart_print!("PAN: enabled via direct MSR\n");
    let current_el: u64;
    unsafe {
        core::arch::asm!(
            "mrs {}, CurrentEL",
            out(reg) current_el,
            options(nomem, nostack)
        );
    }
    crate::uart_print!("CurrentEL=");
    crate::uart_print_usize!(((current_el >> 2) & 3) as usize);
    crate::uart_print!("\n");
    crate::arch::cpu::enable_fp_simd();
    crate::uart_print!("Maya AArch64 booting...\n");
    crate::memory::pmm::init();
    crate::arch::exceptions::init();
    crate::arch::mmu::init();
    let mmfr1: u64;
    unsafe {
        core::arch::asm!(
            "mrs {v}, id_aa64mmfr1_el1",
            v = out(reg) mmfr1,
            options(nomem, nostack)
        );
    }
    let pan_supported = ((mmfr1 >> 20) & 0xF) != 0;
    if pan_supported {
        crate::uart_print!("PAN: supported\n");
    } else {
        crate::uart_print!("PAN: not available\n");
    }
    crate::arch::gic::init();
    crate::arch::timer::init();
    crate::cap::init();
    crate::sched::init();
    crate::uart_print!("Scheduler: init complete\n");
    crate::uart_print!("Scheduler: process count=");
    crate::uart_print_usize!(crate::sched::queue::process_count());
    crate::uart_print!("\n");
    crate::io::mediator::init();
    crate::ipc::init();
    crate::fs::init();
    crate::net::init();
    crate::input::init();
    if crate::gpu::init() {
        crate::gpu::canvas::draw_static_layout();
        crate::gpu::flush_all();
        crate::uart_print!("GPU: canvas ready\n");
    }
    crate::proc::init();
    crate::uart_print!("Process table initialised\n");
    crate::uart_print!("Maya AArch64 ready\n");
    crate::arch::psci::start_all_aps();
    crate::cap::fuzz::run_fuzz_suite();
    crate::io::fuzz::run_fuzz_suite();
    crate::ipc::fuzz::run_fuzz_suite();
    let compute_pid = crate::proc::launch_agentic_aarch64(
        "compute",
        COMPUTE_MEXE,
        COMPUTE_MSHM,
        COMPUTE_MLMB,
        false,
    )
    .expect("compute launch");
    crate::proc::add_process_to_core(
        2,
        compute_pid,
        crate::sched::process::ProcessClass::Interactive,
        1.0,
    );
    let io_pid = crate::proc::launch_agentic_aarch64("io", IO_MEXE, IO_MSHM, IO_MLMB, false)
        .expect("io launch");
    crate::proc::add_process_to_core(
        3,
        io_pid,
        crate::sched::process::ProcessClass::Interactive,
        1.0,
    );
    let bg_pid = crate::proc::launch_agentic_aarch64("bg", BG_MEXE, BG_MSHM, BG_MLMB, false)
        .expect("background launch");
    crate::proc::add_process_to_core(
        4,
        bg_pid,
        crate::sched::process::ProcessClass::Batch,
        0.5,
    );
    let matrix_pid = crate::proc::launch_agentic_aarch64(
        "matrix",
        MATRIX_MEXE,
        MATRIX_MSHM,
        MATRIX_MLMB,
        false,
    )
    .expect("matrix launch");
    crate::proc::add_process_to_core(
        5,
        matrix_pid,
        crate::sched::process::ProcessClass::Interactive,
        1.0,
    );
    let net_pid = crate::proc::launch_agentic_aarch64(
        "net_parser",
        NET_MEXE,
        NET_MSHM,
        NET_MLMB,
        false,
    )
    .expect("net launch");
    crate::proc::add_process_to_core(
        6,
        net_pid,
        crate::sched::process::ProcessClass::Interactive,
        1.0,
    );
    let sort_pid = crate::proc::launch_agentic_aarch64(
        "sort_suite",
        SORT_MEXE,
        SORT_MSHM,
        SORT_MLMB,
        false,
    )
    .expect("sort launch");
    crate::proc::add_process_to_core(
        7,
        sort_pid,
        crate::sched::process::ProcessClass::Batch,
        0.5,
    );
    let mrt_pid = crate::proc::launch_agentic_aarch64(
        "mrt_hello",
        MRT_HELLO_MEXE,
        MRT_HELLO_MSHM,
        MRT_HELLO_MLMB,
        false,
    )
    .expect("mrt_hello launch");
    crate::proc::add_process_to_core(
        1,
        mrt_pid,
        crate::sched::process::ProcessClass::Realtime,
        2.0,
    );
    let producer_pid = crate::proc::launch_agentic_aarch64(
        "mrt_producer",
        MRT_PRODUCER_MEXE,
        MRT_PRODUCER_MSHM,
        MRT_PRODUCER_MLMB,
        false,
    )
    .expect("mrt_producer launch");
    let consumer_pid = crate::proc::launch_agentic_aarch64(
        "mrt_consumer",
        MRT_CONSUMER_MEXE,
        MRT_CONSUMER_MSHM,
        MRT_CONSUMER_MLMB,
        false,
    )
    .expect("mrt_consumer launch");

    let (_sender_cap, _receiver_cap) =
        crate::ipc::channel::create_channel(producer_pid, consumer_pid)
            .expect("producer-consumer channel");
    let (_rev_sender_cap, _rev_receiver_cap) =
        crate::ipc::channel::create_channel(consumer_pid, producer_pid)
            .expect("reverse channel");
    crate::proc::add_process_to_core(
        0,
        producer_pid,
        crate::sched::process::ProcessClass::Realtime,
        2.0,
    );
    crate::proc::add_process_to_core(
        0,
        consumer_pid,
        crate::sched::process::ProcessClass::Realtime,
        2.0,
    );
    let logger_pid = crate::proc::launch_agentic_aarch64(
        "mrt_logger",
        MRT_LOGGER_MEXE,
        MRT_LOGGER_MSHM,
        MRT_LOGGER_MLMB,
        false,
    )
    .expect("mrt_logger launch");
    crate::proc::add_process_to_core(
        1,
        logger_pid,
        crate::sched::process::ProcessClass::Interactive,
        1.0,
    );
    let shell_pid = crate::proc::launch_agentic_aarch64(
        "mrt_shell",
        MRT_SHELL_MEXE,
        MRT_SHELL_MSHM,
        MRT_SHELL_MLMB,
        false,
    )
    .expect("mrt_shell launch");
    crate::proc::add_process_to_core(
        1,
        shell_pid,
        crate::sched::process::ProcessClass::Interactive,
        1.0,
    );

    crate::uart_print!("IPC channel created: producer=");
    crate::uart_print_usize!(producer_pid as usize);
    crate::uart_print!(" consumer=");
    crate::uart_print_usize!(consumer_pid as usize);
    crate::uart_print!("\n");
    crate::uart_print!("Reverse channel: consumer=");
    crate::uart_print_usize!(consumer_pid as usize);
    crate::uart_print!(" producer=");
    crate::uart_print_usize!(producer_pid as usize);
    crate::uart_print!("\n");
    crate::uart_print!("Core assignments:\n");
    crate::uart_print!("  Core 0: producer+consumer\n");
    crate::uart_print!("  Core 1: mrt_hello+mrt_logger+mrt_shell\n");
    crate::uart_print!("  Core 2: compute\n");
    crate::uart_print!("  Core 3: io\n");
    crate::uart_print!("  Core 4: background\n");
    crate::uart_print!("  Core 5: matrix\n");
    crate::uart_print!("  Core 6: net\n");
    crate::uart_print!("  Core 7: sort\n");

    crate::uart_print!("11 processes launched\n");
    crate::gpu::canvas::render_frame();
    crate::gpu::flush_all();
    let process_count = crate::sched::queue::process_count();
    crate::uart_print!("Scheduler queue count=");
    crate::uart_print_usize!(process_count);
    crate::uart_print!("\n");
    crate::sched::queue::debug_print_queue();

    let first_pid = producer_pid;
    crate::uart_print!("First pid=");
    crate::uart_print_usize!(first_pid as usize);
    crate::uart_print!("\n");
    let (entry, stack, ttbr0, asid) =
        crate::proc::get_process_launch_params(first_pid).expect("process params");
    let frame_ptr = crate::proc::get_process_frame(first_pid);
    unsafe {
        core::arch::asm!(
            "msr tpidr_el0, {pid}",
            "msr tpidr_el1, {v}",
            pid = in(reg) first_pid as u64,
            v = in(reg) frame_ptr as u64,
            options(nomem, nostack)
        );
    }
    crate::proc::set_current_proc(0, first_pid);
    crate::proc::set_current_pid(first_pid);

    unsafe {
        crate::proc::jump_to_el0(entry, stack, ttbr0, asid);
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe {
        let dr = 0xFFFF_0000_0900_0000u64 as *mut u32;
        for &b in b"PANIC\r\n" {
            dr.write_volatile(b as u32);
        }
    }
    loop {
        unsafe {
            core::arch::asm!(
                "wfe",
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}
