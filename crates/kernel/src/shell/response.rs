#![allow(dead_code)]
#![allow(static_mut_refs)]

const RESPONSE_BUF_SIZE: usize = 2048;
static mut RESPONSE_BUFFER: [u8; RESPONSE_BUF_SIZE] = [0u8; RESPONSE_BUF_SIZE];

pub fn read_response_blocking() -> &'static str {
    let buf = unsafe { &mut RESPONSE_BUFFER };
    let mut pos = 0usize;
    let end_marker = b"MAYA_RESPONSE_END\n";

    loop {
        let lsr: u8;
        unsafe {
            core::arch::asm!(
                "in al, dx",
                in("dx") 0x3FDu16,
                out("al") lsr,
                options(nomem, nostack, preserves_flags)
            );
        }
        if lsr & 0x01 == 0 {
            continue;
        }

        let byte: u8;
        unsafe {
            core::arch::asm!(
                "in al, dx",
                in("dx") 0x3F8u16,
                out("al") byte,
                options(nomem, nostack, preserves_flags)
            );
        }

        if pos < RESPONSE_BUF_SIZE {
            buf[pos] = byte;
            pos += 1;
        } else {
            break;
        }

        if pos >= end_marker.len() && &buf[pos - end_marker.len()..pos] == end_marker {
            break;
        }
    }

    let full = unsafe { core::str::from_utf8(&RESPONSE_BUFFER[..pos]).unwrap_or("parse error") };

    if let Some(start) = full.find("MAYA_RESPONSE_START\n") {
        let content_start = start + "MAYA_RESPONSE_START\n".len();
        let content = &full[content_start..];
        if let Some(end) = content.find("\nMAYA_RESPONSE_END") {
            let response = &content[..end];
            let response_bytes = response.as_bytes();
            let copy_len = response_bytes.len().min(RESPONSE_BUF_SIZE - 1);
            unsafe {
                static mut RESP_OUT: [u8; 1024] = [0u8; 1024];
                let copy_len = copy_len.min(1023);
                RESP_OUT[..copy_len].copy_from_slice(&response_bytes[..copy_len]);
                RESP_OUT[copy_len] = 0;
                return core::str::from_utf8(&RESP_OUT[..copy_len]).unwrap_or("utf8 error");
            }
        }
    }

    "no response"
}
