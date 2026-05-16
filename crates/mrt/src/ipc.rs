use crate::sys;

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SensorReading {
    pub sensor_id: u8,
    pub value: i16,
    pub timestamp: u32,
    pub flags: u8,
}

impl SensorReading {
    pub fn to_bytes(self) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        bytes[0] = self.sensor_id;
        bytes[1..3].copy_from_slice(&self.value.to_le_bytes());
        bytes[3..7].copy_from_slice(&self.timestamp.to_le_bytes());
        bytes[7] = self.flags;
        bytes
    }

    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        Self {
            sensor_id: bytes[0],
            value: i16::from_le_bytes([bytes[1], bytes[2]]),
            timestamp: u32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]),
            flags: bytes[7],
        }
    }
}

pub struct MrtChannel {
    cap: u128,
}

impl MrtChannel {
    pub fn from_cap_lo(cap_lo: u64) -> MrtChannel {
        MrtChannel { cap: cap_lo as u128 }
    }

    pub fn lookup() -> Option<MrtChannel> {
        unsafe {
            let (lo, hi) = sys::syscall3(0x123, 0, 0, 0);
            if lo < 0 {
                return None;
            }
            Some(MrtChannel {
                cap: (hi as u128) << 64 | lo as u128,
            })
        }
    }

    pub fn lookup_send() -> Option<MrtChannel> {
        unsafe {
            let (lo, hi) = sys::syscall3(0x124, 0, 0, 0);
            if lo < 0 {
                return None;
            }
            Some(MrtChannel {
                cap: (hi as u128) << 64 | lo as u128,
            })
        }
    }

    pub fn create() -> Option<MrtChannel> {
        unsafe {
            let (lo, hi) = sys::syscall3(0x114, 256, 0, 0);
            if lo < 0 {
                return None;
            }
            Some(MrtChannel {
                cap: (hi as u128) << 64 | lo as u128,
            })
        }
    }

    pub fn send(&self, data: &[u8]) -> bool {
        unsafe {
            sys::syscall3(0x21, self.cap as u64, data.as_ptr() as u64, data.len() as u64).0 >= 0
        }
    }

    pub fn recv(&self, buf: &mut [u8]) -> isize {
        unsafe {
            sys::syscall3(0x22, self.cap as u64, buf.as_mut_ptr() as u64, buf.len() as u64).0
                as isize
        }
    }
}
