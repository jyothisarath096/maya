use crate::sys;

#[repr(u64)]
#[derive(Clone, Copy)]
pub enum IntentClass {
    Unknown = 0,
    Compute = 1,
    IO = 2,
    RealTime = 3,
    Background = 4,
    System = 5,
}

pub fn register(name: &[u8], class: IntentClass) -> Option<u128> {
    unsafe {
        let (lo, hi) = sys::syscall3(0x80, name.as_ptr() as u64, name.len() as u64, class as u64);
        if lo < 0 {
            return None;
        }
        Some((hi as u128) << 64 | lo as u128)
    }
}

pub fn telemetry(intent_id: u16) {
    unsafe {
        let _ = sys::syscall0(0x88 | ((intent_id as u64) << 8));
    }
}
