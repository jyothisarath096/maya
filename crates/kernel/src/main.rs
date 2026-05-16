#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

extern crate alloc;

use bootloader_api::config::Mapping;
use bootloader_api::{entry_point, BootInfo, BootloaderConfig};
use core::alloc::Layout;
use core::arch::asm;
use core::panic::PanicInfo;

mod arch;
mod cap;
mod context;
mod fb;
mod fs;
mod ipc;
mod io;
mod memory;
mod model;
mod perf;
mod proc;
mod sched;
mod shell;
mod sync;

const SERIAL_PORT: u16 = 0x3F8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelError {
    ArchInitFailed,
    MemoryInitFailed,
    CapabilityInitFailed,
    IpcInitFailed,
    PmmOutOfMemory,
    PmmDoubleFree,
    PmmInvalidFrame,
    PmmUninitialized,
    PmmBitmapUnavailable,
    PmmVerificationFailed,
    VmmNotInitialized,
    VmmMapFailed,
    VmmUnmapFailed,
    HeapInitFailed,
    CapNotInitialized,
    CapTableFull,
    CapInvalidToken,
    CapInsufficientRights,
    IpcNotInitialized,
    IpcChannelFull,
    IpcChannelEmpty,
    IpcChannelClosed,
    IpcInvalidChannel,
    FsNotInitialized,
    FsFileNotFound,
    FsNotADirectory,
    FsNotAFile,
    FsPermissionDenied,
    FsAlreadyExists,
    FsInvalidPath,
    InvalidElf,
    ProcessError,
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    serial_print("KERNEL PANIC\n");
    fb_print("KERNEL PANIC\n");
    loop {
        unsafe {
            core::arch::asm!(
                "hlt",
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    serial_print("ALLOC ERROR\n");
    halt_loop();
}

static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

static HELLO_ASM_ELF: &[u8] = include_bytes!("proc/hello_asm.bin");
static HELLO_RUST_ELF: &[u8] = include_bytes!("proc/hello_rust.bin");
static HELLO_C_ELF: &[u8] = include_bytes!("proc/hello_c.bin");
static HELLO_MEXE: &[u8] = include_bytes!("proc/hello_mexe.bin");
static HELLO_MSHM: &[u8] = include_bytes!("proc/hello_mshm.bin");
static YIELD_TEST: &[u8] = include_bytes!("proc/yield_test.bin");
static mut LAUNCH_IDX: usize = 0;

#[unsafe(no_mangle)]
pub extern "C" fn run_userspace_tests() {
    loop {
        let idx = unsafe { LAUNCH_IDX };
        unsafe { LAUNCH_IDX += 1; }
        match idx {
            0 => {
                crate::proc::launch("hello_asm", HELLO_ASM_ELF).ok();
            }
            1 => {
                crate::proc::launch("hello_rust", HELLO_RUST_ELF).ok();
            }
            2 => {
                crate::proc::launch("hello_c", HELLO_C_ELF).ok();
            }
            3 => {
                crate::proc::launch("yield_test", YIELD_TEST).ok();
            }
            4 => {
                serial_print("=== Phase 9: MAR Test ===\n");
                fb_print("=== Phase 9: MAR Test ===\n");
                crate::proc::launch_agentic(
                    "hello_mexe",
                    HELLO_MEXE,
                    HELLO_MSHM,
                )
                .ok();
            }
            _ => break,
        }
    }
    serial_print("=== Phase 7: Complete ===\n");
    fb_print("=== Phase 7: Complete ===\n");
    crate::shell::run_interactive();
}

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    if let Some(fb) = boot_info.framebuffer.as_mut() {
        crate::fb::init(fb);
    }
    serial_print("Maya booting...\n");
    fb_print("Maya booting...\n");
    arch::secure::verify_kernel_integrity();
    arch::init().unwrap();
    memory::init(boot_info).unwrap();
    cap::init();
    serial_print("capability system initialised\n");
    fb_print("capability system initialised\n");
    #[cfg(debug_assertions)]
    cap::fuzz::run_fuzz_suite();
    ipc::init();
    serial_print("IPC initialised\n");
    fb_print("IPC initialised\n");
    #[cfg(debug_assertions)]
    io::fuzz::run_fuzz_suite();
    fs::init();
    model::init();
    serial_print("AI model loaded\n");
    fb_print("AI model loaded\n");
    sched::init();
    sched::queue::init_core(0);
    arch::smp::start_aps();
    shell::init();
    #[cfg(debug_assertions)]
    perf::bench::run_smp_benchmarks();
    proc::init();
    proc::syscall::init();
    let cores = sched::queue::online_core_count();
    serial_print("SMP: ");
    serial_print_usize(cores);
    serial_print(" core(s) online\n");
    fb_print("SMP: ");
    fb_print_usize(cores);
    fb_print(" core(s) online\n");
    serial_print("Maya kernel ready\n");
    fb_print("Maya kernel ready\n");
    serial_print("=== Phase 7: Userspace Test ===\n");
    fb_print("=== Phase 7: Userspace Test ===\n");
    crate::proc::set_main_loop_return(run_userspace_tests as *const () as u64);
    run_userspace_tests();
    halt_loop();
}

pub(crate) fn serial_print(s: &str) {
    for byte in s.bytes() {
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") SERIAL_PORT,
                in("al") byte,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

pub(crate) fn fb_print(s: &str) {
    fb::print(s);
}

pub(crate) fn serial_print_usize(mut value: usize) {
    let mut buffer = [0u8; 20];
    let mut index = buffer.len();

    if value == 0 {
        serial_print("0");
        return;
    }

    while value > 0 {
        index -= 1;
        buffer[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    if let Ok(s) = core::str::from_utf8(&buffer[index..]) {
        serial_print(s);
    }
}

pub(crate) fn fb_print_usize(mut value: usize) {
    let mut buffer = [0u8; 20];
    let mut index = buffer.len();

    if value == 0 {
        fb_print("0");
        return;
    }

    while value > 0 {
        index -= 1;
        buffer[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    if let Ok(s) = core::str::from_utf8(&buffer[index..]) {
        fb_print(s);
    }
}

pub(crate) fn halt_loop() -> ! {
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
