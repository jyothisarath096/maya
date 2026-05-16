use core::sync::atomic::{AtomicBool, Ordering};

const UART_BASE: u64 = 0xFFFF_0000_0900_0000;
pub static UART_LOCK: AtomicBool = AtomicBool::new(false);

fn lock() {
    while UART_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
}

fn unlock() {
    UART_LOCK.store(false, Ordering::Release);
}

fn write_bytes_locked(bytes: &[u8]) {
    for &byte in bytes {
        write_byte(byte);
    }
}

pub fn with_lock<F: FnOnce()>(f: F) {
    lock();
    f();
    unlock();
}

pub fn init() {
    unsafe {
        let base = UART_BASE as *mut u32;
        let cr = base.add(0x30 / 4);
        let current = cr.read_volatile();
        cr.write_volatile(current | (1 << 0) | (1 << 8) | (1 << 9));
    }
}

pub fn write_byte(byte: u8) {
    unsafe {
        let base = UART_BASE as *mut u32;
        while base.add(0x18 / 4).read_volatile() & (1 << 5) != 0 {}
        base.write_volatile(byte as u32);
    }
}

pub fn read_byte_nonblocking() -> Option<u8> {
    unsafe {
        let base = UART_BASE as *const u32;
        let fr = base.add(0x18 / 4).read_volatile();
        if fr & (1 << 4) != 0 {
            return None;
        }
        Some(base.read_volatile() as u8)
    }
}

#[inline(never)]
pub fn write_str(s: &str) {
    lock();
    for byte in s.bytes() {
        if byte == b'\n' {
            write_byte(b'\r');
        }
        write_byte(byte);
    }
    unlock();
}

pub fn uart_shell_print(s: &str) {
    lock();
    write_byte(0x01);
    write_bytes_locked(b"SHELL ");
    for byte in s.bytes() {
        if byte == b'\n' || byte == b'\r' {
            write_byte(b' ');
        } else {
            write_byte(byte);
        }
    }
    write_byte(b'\n');
    unlock();
}

#[inline(never)]
pub fn print_hex(val: u64) {
    let mut buf = [0u8; 16];
    for i in 0..16 {
        let nibble = ((val >> (60 - i * 4)) & 0xF) as u8;
        buf[i] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + nibble - 10
        };
    }
    lock();
    for &b in &buf {
        unsafe {
            let dr = UART_BASE as *mut u32;
            dr.write_volatile(b as u32);
        }
    }
    unlock();
}

#[inline(never)]
pub fn print_usize(val: usize) {
    with_lock(|| {
        if val == 0 {
            let dr = UART_BASE as *mut u32;
            unsafe {
                dr.write_volatile(b'0' as u32);
            }
            return;
        }
        let mut buf = [0u8; 20];
        let mut i = 20usize;
        let mut n = val;
        while n > 0 {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
        unsafe {
            let dr = UART_BASE as *mut u32;
            for &b in &buf[i..] {
                dr.write_volatile(b as u32);
            }
        }
    });
}

#[macro_export]
macro_rules! uart_print {
    ($s:expr) => {
        $crate::uart::write_str($s)
    };
}

#[macro_export]
macro_rules! uart_print_hex {
    ($val:expr) => {
        $crate::uart::print_hex($val)
    };
}

#[macro_export]
macro_rules! uart_print_usize {
    ($val:expr) => {
        $crate::uart::print_usize($val)
    };
}
