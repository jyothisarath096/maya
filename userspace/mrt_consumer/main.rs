#![no_std]
#![no_main]

use mrt::fs::MayaFile;
use mrt::intent::{self, IntentClass};
use mrt::io::MrtFile;
use mrt::ipc::{MrtChannel, SensorReading};
use mrt::thread;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    mrt_consumer_main();
    loop {
        thread::yield_now();
    }
}

#[inline(never)]
fn mrt_consumer_main() {
    let _cap = intent::register(b"mrt_consumer", IntentClass::RealTime);
    let mut stdout = MrtFile::stdout().unwrap_or_else(|| panic!());
    let channel = loop {
        if let Some(channel) = MrtChannel::lookup() {
            break channel;
        }
        thread::yield_now();
    };
    let rev_channel = loop {
        if let Some(channel) = MrtChannel::lookup_send() {
            break channel;
        }
        thread::yield_now();
    };
    let mut counts = [0u32; 4];
    let mut alarms = [0u32; 4];
    let mut total = 0u32;
    loop {
        let mut buf = [0u8; 8];
        let n = channel.recv(&mut buf);
        if n > 0 {
            let reading = SensorReading::from_bytes(buf);
            let id = (reading.sensor_id as usize) % 4;
            counts[id] = counts[id].wrapping_add(1);
            if reading.flags & 0x02 != 0 {
                alarms[id] = alarms[id].wrapping_add(1);
                let ack = SensorReading {
                    sensor_id: reading.sensor_id,
                    value: reading.value,
                    timestamp: reading.timestamp,
                    flags: 0x80,
                };
                if rev_channel.send(&ack.to_bytes()) {
                    stdout.write(b"ACK\n");
                } else {
                    stdout.write(b"ACK_FAIL\n");
                }
            }
            total = total.wrapping_add(1);

            if total % 4 == 0 {
                let mut line = [0u8; 6];
                line[..4].copy_from_slice(b"RCV,");
                line[4] = b'0' + (total % 10) as u8;
                line[5] = b'\n';
                stdout.write(&line);
            }

            if total % 16 == 0 {
                if let Some(sensor_file) = MayaFile::open(b"/data/sensors", false) {
                    let mut fbuf = [0u8; 8];
                    let nread = sensor_file.read(&mut fbuf);
                    if nread >= 8 {
                        let fr = SensorReading::from_bytes(fbuf);
                        if fr.sensor_id == reading.sensor_id {
                            stdout.write(b"VFS\n");
                        } else {
                            stdout.write(b"VFS_MISMATCH\n");
                        }
                    }
                }
            }

            if total % 200 == 0 {
                if let Some(shared_file) = MayaFile::open(b"/data/shared", false) {
                    let mut buf = [0u8; 12];
                    let nread = shared_file.read(&mut buf);
                    if nread >= 7 && &buf[..7] == b"SHARED:" {
                        stdout.write(b"SHR\n");
                    }
                }
            }
        }
        thread::yield_now();
    }
}
