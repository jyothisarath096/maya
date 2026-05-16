static FONT: &[u8] = include_bytes!("font8x16.bin");

pub const CHAR_W: u32 = 8;
pub const CHAR_H: u32 = 16;
pub const FONT_FIRST: u8 = 32;
pub const FONT_LAST: u8 = 126;

pub fn draw_char(
    x: u32,
    y: u32,
    ch: u8,
    fg: (u8, u8, u8),
    bg: Option<(u8, u8, u8)>,
) {
    let idx = if (FONT_FIRST..=FONT_LAST).contains(&ch) {
        (ch - FONT_FIRST) as usize
    } else {
        0
    };
    let offset = idx * CHAR_H as usize;

    for row in 0..CHAR_H {
        let byte = FONT[offset + row as usize];
        for col in 0..CHAR_W {
            let bit = (byte >> (7 - col)) & 1;
            let px = x + col;
            let py = y + row;
            if bit != 0 {
                crate::gpu::driver::set_pixel_raw(px, py, fg.0, fg.1, fg.2);
            } else if let Some(b) = bg {
                crate::gpu::driver::set_pixel_raw(px, py, b.0, b.1, b.2);
            }
        }
    }
}

pub fn draw_str(
    x: u32,
    y: u32,
    s: &[u8],
    fg: (u8, u8, u8),
    bg: Option<(u8, u8, u8)>,
) -> u32 {
    let mut cx = x;
    for &ch in s {
        if ch == b'\n' {
            break;
        }
        if cx + CHAR_W > crate::gpu::driver::FB_WIDTH {
            break;
        }
        draw_char(cx, y, ch, fg, bg);
        cx += CHAR_W;
    }
    cx
}

pub fn draw_u64(
    x: u32,
    y: u32,
    val: u64,
    fg: (u8, u8, u8),
    bg: Option<(u8, u8, u8)>,
) -> u32 {
    let mut buf = [0u8; 20];
    let mut len = 0;
    let mut v = val;
    if v == 0 {
        buf[0] = b'0';
        len = 1;
    } else {
        while v > 0 && len < buf.len() {
            buf[len] = b'0' + (v % 10) as u8;
            v /= 10;
            len += 1;
        }
        buf[..len].reverse();
    }
    draw_str(x, y, &buf[..len], fg, bg)
}

pub fn draw_hex(
    x: u32,
    y: u32,
    val: u64,
    digits: usize,
    fg: (u8, u8, u8),
    bg: Option<(u8, u8, u8)>,
) -> u32 {
    let mut buf = [b'0'; 16];
    for i in 0..digits.min(buf.len()) {
        let nibble = ((val >> ((digits - 1 - i) * 4)) & 0xF) as u8;
        buf[i] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'A' + nibble - 10
        };
    }
    draw_str(x, y, &buf[..digits.min(buf.len())], fg, bg)
}
