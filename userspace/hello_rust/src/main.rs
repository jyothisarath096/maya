#![no_std]
#![no_main]

const SYS_EXIT: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_GETCAP: u64 = 5;

fn syscall1(nr: u64, arg1: u64) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") nr as i64 => ret,
            in("rdi") arg1,
            out("rcx") _,
            out("r11") _,
        );
    }
    ret
}

fn syscall3(nr: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") nr as i64 => ret,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            out("rcx") _,
            out("r11") _,
        );
    }
    ret
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let stdout = syscall1(SYS_GETCAP, 0) as u64;
    let msg = b"Hello from Maya Rust userspace!\n";
    syscall3(SYS_WRITE, stdout, msg.as_ptr() as u64, msg.len() as u64);
    syscall1(SYS_EXIT, 0);
    loop {}
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
