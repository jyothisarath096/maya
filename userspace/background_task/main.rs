#![no_std]
#![no_main]

use core::hint::black_box;

use mrt::intent::{self, IntentClass};
use mrt::thread;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    mrt_background_main();
    loop {
        thread::yield_now();
    }
}

#[inline(never)]
fn mrt_background_main() {
    let _cap = intent::register(b"mrt_background", IntentClass::Background);

    let mut input = [0u8; 128];
    let mut output = [0u8; 256];
    let mut seed: u32 = 0x1234_5678;

    loop {
        intent::telemetry(102);
        let mut i = 0usize;
        while i < 128 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let run_len = 1 + (seed & 0x7) as usize;
            let val = (seed >> 8) as u8;
            let end = (i + run_len).min(128);
            while i < end {
                input[i] = val;
                i += 1;
            }
        }

        let mut out_len = 0usize;
        let mut in_idx = 0usize;
        while in_idx < 128 && out_len + 2 < 256 {
            let val = input[in_idx];
            let mut run = 1usize;
            while in_idx + run < 128 && run < 255 && input[in_idx + run] == val {
                run += 1;
            }
            output[out_len] = run as u8;
            output[out_len + 1] = val;
            out_len += 2;
            in_idx += run;
        }
        seed ^= out_len as u32;
        black_box((out_len, output[0], seed));
        thread::yield_now();
    }
}
