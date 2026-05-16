#![no_std]
#![no_main]

use core::hint::black_box;

use mrt::intent::{self, IntentClass};
use mrt::thread;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    mrt_compute_main();
    loop {
        thread::yield_now();
    }
}

#[inline(never)]
fn mrt_compute_main() {
    let _cap = intent::register(b"mrt_compute", IntentClass::Compute);

    let mut data = [0i32; 16];
    for i in 0..16 {
        data[i] = (i as i32 + 1) * 3;
    }

    let mut iter: u32 = 0;
    loop {
        intent::telemetry(100);
        let mut acc: i64 = 0;
        for _ in 0..1000 {
            for i in 0..16 {
                acc = acc.wrapping_add(data[i] as i64 * data[(i + 1) % 16] as i64);
            }
        }
        data[0] = (acc & 0xFF) as i32 + 1;
        iter = iter.wrapping_add(((acc as u32) & 0xF).wrapping_add(1));
        black_box((acc, iter, data[0]));
        thread::yield_now();
    }
}
