use crate::sys;

pub struct MrtFile {
    cap: u128,
    offset: usize,
}

impl MrtFile {
    pub fn stdout() -> Option<MrtFile> {
        unsafe {
            let (cap_lo, cap_hi) = crate::sys::syscall3(0x115, 0, 0, 0);
            Some(MrtFile {
                cap: if cap_lo < 0 {
                    0
                } else {
                    (cap_hi as u128) << 64 | cap_lo as u128
                },
                offset: 0,
            })
        }
    }

    pub fn open(path: &[u8]) -> Option<MrtFile> {
        unsafe {
            let (cap_lo, cap_hi) =
                sys::syscall3(0x102, path.as_ptr() as u64, path.len() as u64, 0);
            if cap_lo < 0 {
                return None;
            }
            Some(MrtFile {
                cap: (cap_hi as u128) << 64 | cap_lo as u128,
                offset: 0,
            })
        }
    }

    pub fn read(&mut self, buf: &mut [u8]) -> isize {
        unsafe {
            let (ret, _) = sys::syscall3(
                0x100,
                self.cap as u64,
                buf.as_mut_ptr() as u64,
                buf.len() as u64,
            );
            self.offset = self.offset.saturating_add(ret.max(0) as usize);
            ret as isize
        }
    }

    pub fn write(&mut self, buf: &[u8]) -> isize {
        unsafe {
            let (ret, _) = sys::syscall3(
                0x101,
                self.cap as u64,
                buf.as_ptr() as u64,
                buf.len() as u64,
            );
            self.offset = self.offset.saturating_add(ret.max(0) as usize);
            ret as isize
        }
    }

    pub fn write_shell_frame(&mut self, text: &[u8]) -> isize {
        let mut framed = [0u8; 264];
        let prefix = b"\x01SHELL ";
        let max_text = framed.len().saturating_sub(prefix.len() + 1);
        let text_len = text.len().min(max_text);
        framed[..prefix.len()].copy_from_slice(prefix);
        framed[prefix.len()..prefix.len() + text_len].copy_from_slice(&text[..text_len]);
        framed[prefix.len() + text_len] = b'\n';
        self.write(&framed[..prefix.len() + text_len + 1])
    }
}

pub fn shell_print(s: &str) {
    if let Some(mut stdout) = MrtFile::stdout() {
        let _ = stdout.write_shell_frame(s.as_bytes());
    }
}
