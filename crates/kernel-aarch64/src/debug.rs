#[macro_export]
macro_rules! kdbg {
    ($msg:expr) => {
        #[cfg(feature = "debug_log")]
        {
            core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
            let sp: u64;
            unsafe {
                core::arch::asm!(
                    "mov {sp}, sp",
                    sp = out(reg) sp,
                    options(nomem, nostack, preserves_flags)
                );
            }
            $crate::uart_print!("[DBG sp=");
            $crate::uart_print_hex!(sp);
            $crate::uart_print!(" ");
            $crate::uart_print!(file!());
            $crate::uart_print!(":");
            $crate::uart_print_usize!(line!() as usize);
            $crate::uart_print!("] ");
            $crate::uart_print!($msg);
            $crate::uart_print!("\n");
            core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        }
    };
    ($msg:expr, $val:expr) => {
        #[cfg(feature = "debug_log")]
        {
            core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
            let sp: u64;
            unsafe {
                core::arch::asm!(
                    "mov {sp}, sp",
                    sp = out(reg) sp,
                    options(nomem, nostack, preserves_flags)
                );
            }
            $crate::uart_print!("[DBG sp=");
            $crate::uart_print_hex!(sp);
            $crate::uart_print!(" ");
            $crate::uart_print!(file!());
            $crate::uart_print!(":");
            $crate::uart_print_usize!(line!() as usize);
            $crate::uart_print!("] ");
            $crate::uart_print!($msg);
            $crate::uart_print_usize!($val as usize);
            $crate::uart_print!("\n");
            core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        }
    };
}
