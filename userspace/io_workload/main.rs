#![no_std]
#![no_main]

use core::hint::black_box;

use mrt::intent::{self, IntentClass};
use mrt::thread;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    mrt_io_main();
    loop {
        thread::yield_now();
    }
}

#[inline(never)]
fn mrt_io_main() {
    let _cap = intent::register(b"mrt_io", IntentClass::IO);

    let mut buf = [0u8; 64];
    let mut seed: u32 = 0xDEAD_BEEF;

    loop {
        intent::telemetry(101);
        for byte in &mut buf {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            *byte = (seed >> 16) as u8;
        }

        let wait = 2000 + (seed & 0x0FFF);
        let mut w = wait;
        while w > 0 {
            w -= 1;
        }

        let cksum: u32 = buf.iter().map(|&b| b as u32).sum();
        seed = seed.wrapping_add(cksum.rotate_left(3));
        black_box((buf[0], cksum, seed));
        thread::yield_now();
    }
}
