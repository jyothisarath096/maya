pub fn read_char() -> u8 {
    unsafe { crate::sys::syscall4(0x50, 0, 0, 0, 0).0 as u8 }
}
