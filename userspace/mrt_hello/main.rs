#![no_std]
#![no_main]

use mrt::alloc::MrtAlloc;
use mrt::intent::{self, IntentClass};
use mrt::io::MrtFile;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    mrt_main();
    loop {
        unsafe {
            core::arch::asm!(
                "mov x8, #0x01",
                "svc #0",
                out("x8") _,
                options(nostack)
            );
        }
    }
}

#[inline(never)]
fn mrt_main() {
    let _cap = intent::register(b"mrt_hello", IntentClass::RealTime);

    let mut stdout = MrtFile::stdout().unwrap_or_else(|| panic!());
    let mut count: u32 = 0;

    loop {
        count = count.wrapping_add(1);
        if count % 100 == 0 {
            let _ = stdout.write(b"MRT\n");
        }
        let _alloc = MrtAlloc::alloc(256, 1);
        unsafe {
            core::arch::asm!(
                "mov x8, #0x01",
                "svc #0",
                out("x8") _,
                options(nostack)
            );
        }
    }
}
