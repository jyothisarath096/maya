pub struct MayaNet;

impl MayaNet {
    pub fn send(payload: &[u8], dst_port: u16) -> isize {
        unsafe {
            crate::sys::syscall4(0x40, payload.as_ptr() as u64, payload.len() as u64, dst_port as u64, 0).0
                as isize
        }
    }

    pub fn recv(buf: &mut [u8]) -> isize {
        unsafe {
            crate::sys::syscall4(0x41, buf.as_mut_ptr() as u64, buf.len() as u64, 0, 0).0
                as isize
        }
    }

    pub fn mac(buf: &mut [u8; 6]) {
        unsafe {
            let _ = crate::sys::syscall4(0x42, buf.as_mut_ptr() as u64, 0, 0, 0);
        }
    }
}
