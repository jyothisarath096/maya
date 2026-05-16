#![no_std]
#![no_main]

use core::hint::black_box;

use mrt::intent::{self, IntentClass};
use mrt::io::MrtFile;
use mrt::net::MayaNet;
use mrt::thread;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    mrt_net_main();
    loop {
        thread::yield_now();
    }
}

#[inline(never)]
fn mrt_net_main() {
    let _cap = intent::register(b"mrt_net", IntentClass::IO);
    let mut stdout = match MrtFile::stdout() {
        Some(file) => file,
        None => loop {
            thread::yield_now();
        },
    };

    let request =
        b"GET /api/v1/sensor HTTP/1.1\r\nHost: maya.os\r\nContent-Length: 42\r\n\r\n";

    let mut parsed_count: u32 = 0;
    let mut content_length: u32 = 0;
    let mut iter_count: u32 = 0;

    loop {
        intent::telemetry(104);
        let mut method_end = 0usize;
        for (i, &b) in request.iter().enumerate() {
            if b == b' ' {
                method_end = i;
                break;
            }
        }

        let cl_key = b"Content-Length: ";
        let mut cl_val = 0u32;
        let mut found = false;
        let mut i = 0usize;
        while i + cl_key.len() <= request.len() {
            if &request[i..i + cl_key.len()] == cl_key {
                let mut j = i + cl_key.len();
                while j < request.len() && request[j].is_ascii_digit() {
                    cl_val = cl_val
                        .wrapping_mul(10)
                        .wrapping_add((request[j] - b'0') as u32);
                    j += 1;
                }
                found = true;
                break;
            }
            i += 1;
        }

        if found {
            content_length = content_length.wrapping_add(cl_val);
        }
        parsed_count = parsed_count.wrapping_add(1);
        iter_count = iter_count.wrapping_add(1);
        if iter_count % 50 == 0 {
            let msg = b"MAYA:hello\n";
            let n = MayaNet::send(msg, 5555);
            if n > 0 {
                let _ = stdout.write(b"PKT\n");
            }
        }
        let mut rbuf = [0u8; 256];
        let n = MayaNet::recv(&mut rbuf);
        if n > 8 {
            let _ = stdout.write(b"RCV_NET\n");
        }
        black_box((method_end, parsed_count, content_length));
        thread::yield_now();
    }
}
