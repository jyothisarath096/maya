#![no_std]
#![no_main]

use core::hint::black_box;

use mrt::intent::{self, IntentClass};
use mrt::thread;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    mrt_sort_main();
    loop {
        thread::yield_now();
    }
}

#[inline(never)]
fn mrt_sort_main() {
    let _cap = intent::register(b"mrt_sort", IntentClass::Background);

    let mut data = [0i32; 64];
    let mut temp = [0i32; 64];
    let mut seed: u32 = 0xABCD_1234;

    loop {
        intent::telemetry(105);
        for item in &mut data {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            *item = ((seed >> 16) & 0x7FFF) as i32;
        }

        for i in 0..31 {
            for j in 0..(31 - i) {
                if data[j] > data[j + 1] {
                    data.swap(j, j + 1);
                }
            }
        }

        let mut width = 1usize;
        while width < 32 {
            let mut i = 32usize;
            while i < 64 {
                let l = i;
                let m = (i + width).min(64);
                let r = (i + width * 2).min(64);
                temp[l..r].copy_from_slice(&data[l..r]);
                let (mut a, mut b_idx, mut k) = (l, m, l);
                while a < m && b_idx < r {
                    if temp[a] <= temp[b_idx] {
                        data[k] = temp[a];
                        a += 1;
                    } else {
                        data[k] = temp[b_idx];
                        b_idx += 1;
                    }
                    k += 1;
                }
                while a < m {
                    data[k] = temp[a];
                    a += 1;
                    k += 1;
                }
                while b_idx < r {
                    data[k] = temp[b_idx];
                    b_idx += 1;
                    k += 1;
                }
                i += width * 2;
            }
            width *= 2;
        }

        let target = data[32];
        let mut lo = 0i32;
        let mut hi = 63i32;
        while lo <= hi {
            let mid = (lo + hi) / 2;
            let val = data[mid as usize];
            if val == target {
                break;
            } else if val < target {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
        }
        seed ^= (target as u32).wrapping_add(lo as u32);
        black_box((data[0], data[32], seed));
        thread::yield_now();
    }
}
