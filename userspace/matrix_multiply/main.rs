#![no_std]
#![no_main]

use core::hint::black_box;

use mrt::intent::{self, IntentClass};
use mrt::thread;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    mrt_matrix_main();
    loop {
        thread::yield_now();
    }
}

#[inline(never)]
fn mrt_matrix_main() {
    let _cap = intent::register(b"mrt_matrix", IntentClass::Compute);

    let mut a = [[0i32; 8]; 8];
    let mut b = [[0i32; 8]; 8];
    let mut c = [[0i32; 8]; 8];

    for i in 0..8 {
        for j in 0..8 {
            a[i][j] = ((i * 8 + j + 1) % 16) as i32;
            b[i][j] = ((i + j * 3 + 2) % 16) as i32;
        }
    }

    loop {
        intent::telemetry(103);
        for i in 0..8 {
            for j in 0..8 {
                let mut acc = 0i32;
                for k in 0..8 {
                    acc = acc.wrapping_add(a[i][k].wrapping_mul(b[k][j]));
                }
                c[i][j] = acc;
            }
        }

        for i in 0..8 {
            for j in 0..8 {
                a[i][j] = c[i][j] & 0xF;
                b[i][j] = c[(i + 1) % 8][j] & 0xF;
            }
        }
        black_box((a[0][0], b[0][0], c[0][0]));
        thread::yield_now();
    }
}
