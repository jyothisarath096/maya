#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dst: *mut u8, value: i32, len: usize) -> *mut u8 {
    let byte = value as u8;
    let mut i = 0usize;
    while i < len {
        unsafe {
            *dst.add(i) = byte;
        }
        i += 1;
    }
    dst
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dst: *mut u8, src: *const u8, len: usize) -> *mut u8 {
    let mut i = 0usize;
    while i < len {
        unsafe {
            *dst.add(i) = *src.add(i);
        }
        i += 1;
    }
    dst
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dst: *mut u8, src: *const u8, len: usize) -> *mut u8 {
    if (dst as usize) <= (src as usize) || (dst as usize) >= (src as usize).saturating_add(len) {
        unsafe { memcpy(dst, src, len) }
    } else {
        let mut i = len;
        while i > 0 {
            let idx = i - 1;
            unsafe {
                *dst.add(idx) = *src.add(idx);
            }
            i -= 1;
        }
        dst
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(lhs: *const u8, rhs: *const u8, len: usize) -> i32 {
    let mut i = 0usize;
    while i < len {
        let a = unsafe { *lhs.add(i) };
        let b = unsafe { *rhs.add(i) };
        if a != b {
            return a as i32 - b as i32;
        }
        i += 1;
    }
    0
}
