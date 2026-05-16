#![no_std]

pub mod alloc;
pub mod fs;
pub mod input;
pub mod intent;
pub mod io;
pub mod ipc;
mod mem;
pub mod net;
pub mod sys;
pub mod sync;
pub mod thread;

pub use fs::{mkdir, query_by_intent, MayaFile};
pub use io::shell_print;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
