#![no_std]
#![no_main]

use mrt::fs::MayaFile;
use mrt::intent::{self, IntentClass};
use mrt::io::MrtFile;
use mrt::ipc::{MrtChannel, SensorReading};
use mrt::thread;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    mrt_producer_main();
    loop {
        thread::yield_now();
    }
}

#[inline(never)]
fn mrt_producer_main() {
    let _cap = intent::register(b"mrt_producer", IntentClass::RealTime);
    let mut stdout = MrtFile::stdout().unwrap_or_else(|| panic!());
    let channel = loop {
        if let Some(channel) = MrtChannel::lookup_send() {
            break channel;
        }
        thread::yield_now();
    };
    let rev_channel = loop {
        if let Some(channel) = MrtChannel::lookup() {
            break channel;
        }
        thread::yield_now();
    };
    let mut counter: u32 = 0;
    let mut sensor_id: u8 = 0;
    let mut share_counter: u32 = 0;
    loop {
        let value = (counter % 100) as i16;
        let alarm = value > 80;
        let reading = SensorReading {
            sensor_id,
            value,
            timestamp: counter,
            flags: if alarm { 0x03 } else { 0x01 },
        };
        let buf = reading.to_bytes();
        if channel.send(&buf) {
            let mut line = [0u8; 8];
            line[..4].copy_from_slice(b"SND,");
            line[4] = b'0' + sensor_id;
            line[5] = b',';
            line[6] = b'0' + (value % 10) as u8;
            line[7] = b'\n';
            stdout.write(&line);
        }
        if let Some(sensor_file) = MayaFile::open(b"/data/sensors", true) {
            let _ = sensor_file.write(&buf);
        }
        share_counter = share_counter.wrapping_add(1);
        if share_counter == 200 {
            share_counter = 0;
            if let Some(shared) = MayaFile::open(b"/data/shared", true) {
                let mut msg = [0u8; 20];
                msg[..7].copy_from_slice(b"SHARED:");
                msg[7] = b'0' + (counter % 10) as u8;
                msg[8] = b'\n';
                let _ = shared.write(&msg[..9]);
                if shared.delegate(12, true).is_some() {
                    stdout.write(b"DEL\n");
                }
                if shared.revoke() {
                    stdout.write(b"REV\n");
                }
            }
        }
        let mut ack_buf = [0u8; 8];
        if rev_channel.recv(&mut ack_buf) > 0 {
            let ack = SensorReading::from_bytes(ack_buf);
            if ack.flags == 0x80 {
                let mut line = [0u8; 6];
                line[..4].copy_from_slice(b"ALM,");
                line[4] = b'0' + ack.sensor_id;
                line[5] = b'\n';
                stdout.write(&line);
            }
        }
        sensor_id = (sensor_id + 1) % 4;
        counter = counter.wrapping_add(1);
        thread::yield_now();
    }
}
