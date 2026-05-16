#![no_std]
#![no_main]

use mrt::fs::{mkdir, query_by_intent, MayaFile};
use mrt::intent::{self, IntentClass};
use mrt::io::MrtFile;
use mrt::thread;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    mrt_logger_main();
    loop {
        thread::yield_now();
    }
}

#[inline(never)]
fn mrt_logger_main() {
    let _cap = intent::register(b"mrt_logger", IntentClass::IO);
    let mut stdout = match MrtFile::stdout() {
        Some(file) => file,
        None => loop {
            thread::yield_now();
        },
    };

    let _ = mkdir(b"/data");

    let mut counter: u32 = 0;
    let mut yield_count: u32 = 0;
    let mut version_demo_done = false;
    loop {
        yield_count = yield_count.wrapping_add(1);
        if yield_count % 100 == 0 {
            if let Some(file) = MayaFile::open(b"/data/log", true) {
                let mut msg = [0u8; 32];
                msg[..4].copy_from_slice(b"LOG:");
                let n = counter % 10000;
                msg[4] = b'0' + ((n / 1000) % 10) as u8;
                msg[5] = b'0' + ((n / 100) % 10) as u8;
                msg[6] = b'0' + ((n / 10) % 10) as u8;
                msg[7] = b'0' + (n % 10) as u8;
                msg[8] = b'\n';
                let written = file.write(&msg[..9]);

                let mut line = [0u8; 6];
                line[..4].copy_from_slice(b"WFS,");
                line[4] = b'0' + ((written.max(0) as u32) % 10) as u8;
                line[5] = b'\n';
                let _ = stdout.write(&line);

                if let Some(read_file) = MayaFile::open(b"/data/log", false) {
                    let mut rbuf = [0u8; 32];
                    let nread = read_file.read(&mut rbuf);
                    if nread > 0 && rbuf[..4] == *b"LOG:" {
                        let mut rline = [0u8; 6];
                        rline[..4].copy_from_slice(b"RFS,");
                        rline[4] = b'0' + (nread.max(0) as u32 % 10) as u8;
                        rline[5] = b'\n';
                        let _ = stdout.write(&rline);
                    }
                }
                counter = counter.wrapping_add(1);
                if counter >= 10 && !version_demo_done {
                    version_demo_done = true;
                    let _ = stdout.write(b"VX\n");

                    if let Some(read_file) = MayaFile::open(b"/data/log", false) {
                        let _ = stdout.write(b"VY\n");
                        let (total, oldest) = read_file.version_info();

                        let mut vi = [0u8; 8];
                        vi[..3].copy_from_slice(b"VI:");
                        vi[3] = b'0' + (total % 10) as u8;
                        vi[4] = b',';
                        vi[5] = b'0' + (oldest % 10) as u8;
                        vi[6] = b'\n';
                        let _ = stdout.write(&vi[..7]);

                        let mut vbuf = [0u8; 16];
                        let (n1, v1) = read_file.read_version(oldest, &mut vbuf);
                        if n1 > 0 {
                            let mut vl = [0u8; 8];
                            vl[..2].copy_from_slice(b"VA");
                            vl[2] = b'0' + (v1 % 10) as u8;
                            vl[3] = b':';
                            vl[4] = if n1 >= 8 { vbuf[7] } else { b'?' };
                            vl[5] = b'\n';
                            let _ = stdout.write(&vl[..6]);
                        }

                        let mid = oldest + 3;
                        let (n2, v2) = read_file.read_version(mid, &mut vbuf);
                        if n2 > 0 {
                            let mut vl = [0u8; 8];
                            vl[..2].copy_from_slice(b"VB");
                            vl[2] = b'0' + (v2 % 10) as u8;
                            vl[3] = b':';
                            vl[4] = if n2 >= 8 { vbuf[7] } else { b'?' };
                            vl[5] = b'\n';
                            let _ = stdout.write(&vl[..6]);
                        }

                        let (n3, v3) = read_file.read_version(u32::MAX, &mut vbuf);
                        if n3 > 0 {
                            let mut vl = [0u8; 8];
                            vl[..2].copy_from_slice(b"VC");
                            vl[2] = b'0' + (v3 % 10) as u8;
                            vl[3] = b':';
                            vl[4] = if n3 >= 8 { vbuf[7] } else { b'?' };
                            vl[5] = b'\n';
                            let _ = stdout.write(&vl[..6]);
                        }

                        if let Some(file) = MayaFile::open(b"/data/log", true) {
                            let _ = file.tag(b"type=log");
                            let _ = file.tag(b"format=text");
                            let _ = stdout.write(b"TAG\n");
                        }

                        let mut ids = [0u32; 8];
                        let n = query_by_intent(2, &mut ids);
                        if n > 0 {
                            let mut line = [0u8; 6];
                            line[..4].copy_from_slice(b"QRY,");
                            line[4] = b'0' + (n % 10) as u8;
                            line[5] = b'\n';
                            let _ = stdout.write(&line);
                        }

                        if let Some(alias_file) = MayaFile::open(b"/sys/io/log", false) {
                            let mut buf = [0u8; 8];
                            let n = alias_file.read(&mut buf);
                            if n > 0 && &buf[..4] == b"LOG:" {
                                let _ = stdout.write(b"SYS\n");
                            }
                        }
                    } else {
                        let _ = stdout.write(b"VN\n");
                    }
                }
            }
        }
        if yield_count % 200 == 0 {
            if let Some(sched_file) = MayaFile::open(b"/proc/sched", false) {
                let mut buf = [0u8; 64];
                let n = sched_file.read(&mut buf);
                if n > 0 {
                    let _ = stdout.write(b"SCH:");
                    let _ = stdout.write(&buf[..(n as usize).min(12)]);
                    let _ = stdout.write(b"\n");
                }
            }
        }
        if yield_count % 300 == 0 {
            if let Some(proc_file) = MayaFile::open(b"/proc/11/stats", false) {
                let mut buf = [0u8; 128];
                let n = proc_file.read(&mut buf);
                if n > 0 {
                    let _ = stdout.write(b"PST:");
                    let _ = stdout.write(&buf[..(n as usize).min(20)]);
                }
            }
        }
        thread::yield_now();
    }
}
