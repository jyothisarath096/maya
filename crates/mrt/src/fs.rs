pub struct MayaFile {
    cap_lo: u64,
    cap_hi: u64,
    owned: bool,
}

impl MayaFile {
    pub fn open(path: &[u8], write: bool) -> Option<MayaFile> {
        let flags = if write { 1 } else { 0 };
        unsafe {
            let (lo, hi) = crate::sys::syscall4(0x30, path.as_ptr() as u64, path.len() as u64, 1, flags);
            if lo < 0 {
                return None;
            }
            Some(MayaFile {
                cap_lo: lo as u64,
                cap_hi: hi as u64,
                owned: true,
            })
        }
    }

    pub fn read(&self, buf: &mut [u8]) -> isize {
        let _ = self.cap_hi;
        unsafe {
            crate::sys::syscall4(0x31, self.cap_lo, buf.as_mut_ptr() as u64, buf.len() as u64, 0).0
                as isize
        }
    }

    pub fn write(&self, data: &[u8]) -> isize {
        let _ = self.cap_hi;
        unsafe {
            crate::sys::syscall4(0x32, self.cap_lo, data.as_ptr() as u64, data.len() as u64, 0).0
                as isize
        }
    }

    pub fn stat(&self) -> (usize, u32) {
        let _ = self.cap_hi;
        unsafe {
            let (size, ver) = crate::sys::syscall4(0x33, self.cap_lo, 0, 0, 0);
            (size as usize, ver as u32)
        }
    }

    pub fn read_version(&self, version_num: u32, buf: &mut [u8]) -> (isize, u32) {
        let _ = self.cap_hi;
        unsafe {
            let (n, ver) = crate::sys::syscall4(
                0x36,
                self.cap_lo,
                buf.as_mut_ptr() as u64,
                buf.len() as u64,
                version_num as u64,
            );
            (n as isize, ver as u32)
        }
    }

    pub fn version_info(&self) -> (u32, u32) {
        let _ = self.cap_hi;
        unsafe {
            let (count, oldest) = crate::sys::syscall4(0x39, self.cap_lo, 0, 0, 0);
            (count as u32, oldest as u32)
        }
    }

    pub fn delegate(&self, target_pid: u16, read_only: bool) -> Option<(u64, u64)> {
        let rights = if read_only { 0x1 } else { 0x3 };
        unsafe {
            let (lo, hi) = crate::sys::syscall4(0x37, self.cap_lo, target_pid as u64, rights, 0);
            if lo < 0 {
                return None;
            }
            Some((lo as u64, hi as u64))
        }
    }

    pub fn revoke(&self) -> bool {
        unsafe { crate::sys::syscall4(0x38, self.cap_lo, 0, 0, 0).0 >= 0 }
    }

    pub fn tag(&self, kv: &[u8]) -> bool {
        unsafe { crate::sys::syscall4(0x3A, self.cap_lo, kv.as_ptr() as u64, kv.len() as u64, 0).0 >= 0 }
    }

    pub fn from_delegated(cap_lo: u64, cap_hi: u64) -> MayaFile {
        MayaFile {
            cap_lo,
            cap_hi,
            owned: false,
        }
    }
}

impl Drop for MayaFile {
    fn drop(&mut self) {
        if self.owned && self.cap_lo != 0 {
            unsafe {
                let _ = crate::sys::syscall4(0x35, self.cap_lo, 0, 0, 0);
            }
            self.cap_lo = 0;
            self.cap_hi = 0;
        }
    }
}

pub fn mkdir(path: &[u8]) -> bool {
    unsafe { crate::sys::syscall4(0x34, path.as_ptr() as u64, path.len() as u64, 0, 0).0 >= 0 }
}

pub fn query_by_intent(intent: u8, buf: &mut [u32]) -> usize {
    unsafe {
        let (n, _) = crate::sys::syscall4(0x3B, intent as u64, buf.as_mut_ptr() as u64, buf.len() as u64, 0);
        n.max(0) as usize
    }
}
