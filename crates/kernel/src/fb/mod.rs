#![allow(dead_code)]

use bootloader_api::info::{FrameBuffer, PixelFormat};
use spinning_top::Spinlock;

const FONT_WIDTH: usize = 8;
const FONT_HEIGHT: usize = 16;
const FG_COLOR: u32 = 0x00FF_FFFF;
const BG_COLOR: u32 = 0x0000_0000;

struct FbConsole {
    fb_addr: u64,
    fb_width: usize,
    fb_height: usize,
    fb_stride: usize,
    fb_bpp: usize,
    pixel_fmt: PixelFormat,
    col: usize,
    row: usize,
    max_cols: usize,
    max_rows: usize,
}

static CONSOLE: Spinlock<Option<FbConsole>> = Spinlock::new(None);

static FONT: &[u8] = include_bytes!("font8x16.bin");

pub fn init(fb: &mut FrameBuffer) {
    let width = fb.info().width;
    let height = fb.info().height;
    let stride = fb.info().stride;
    let bytes_per_pixel = fb.info().bytes_per_pixel;
    let pixel_format = fb.info().pixel_format;
    let addr = fb.buffer_mut().as_mut_ptr() as u64;
    let max_cols = width / FONT_WIDTH;
    let max_rows = height / FONT_HEIGHT;

    let mut console = CONSOLE.lock();
    *console = Some(FbConsole {
        fb_addr: addr,
        fb_width: width,
        fb_height: height,
        fb_stride: stride,
        fb_bpp: bytes_per_pixel,
        pixel_fmt: pixel_format,
        col: 0,
        row: 0,
        max_cols,
        max_rows,
    });

    if let Some(ref mut c) = *console {
        c.clear();
    }
}

pub fn print(s: &str) {
    let mut console = CONSOLE.lock();
    if let Some(ref mut c) = *console {
        for byte in s.bytes() {
            c.write_byte(byte);
        }
    }
}

impl FbConsole {
    fn clear(&mut self) {
        let fb = self.fb_addr as *mut u8;
        let size = self.fb_stride * self.fb_height * self.fb_bpp;
        unsafe {
            core::ptr::write_bytes(fb, 0, size);
        }
    }

    fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                self.col = 0;
                self.row += 1;
                if self.row >= self.max_rows {
                    self.scroll();
                }
            }
            b'\r' => {
                self.col = 0;
            }
            b'\x08' => {
                if self.col > 0 {
                    self.col -= 1;
                    self.draw_char(b' ');
                }
            }
            32..=126 => {
                self.draw_char(byte);
                self.col += 1;
                if self.col >= self.max_cols {
                    self.col = 0;
                    self.row += 1;
                    if self.row >= self.max_rows {
                        self.scroll();
                    }
                }
            }
            _ => {}
        }
    }

    fn draw_char(&self, ch: u8) {
        if ch < 32 || ch > 126 {
            return;
        }
        let glyph_idx = (ch as usize - 32) * FONT_HEIGHT;
        if glyph_idx + FONT_HEIGHT > FONT.len() {
            return;
        }

        let x = self.col * FONT_WIDTH;
        let y = self.row * FONT_HEIGHT;

        for row in 0..FONT_HEIGHT {
            let glyph_row = FONT[glyph_idx + row];
            for col in 0..FONT_WIDTH {
                let mask = 0x80u8 >> col;
                let set = glyph_row & mask != 0;
                let color = if set { FG_COLOR } else { BG_COLOR };
                self.put_pixel(x + col, y + row, color);
            }
        }
    }

    fn put_pixel(&self, x: usize, y: usize, color: u32) {
        if x >= self.fb_width || y >= self.fb_height {
            return;
        }

        let offset = (y * self.fb_stride + x) * self.fb_bpp;
        let fb = self.fb_addr as *mut u8;
        let r = ((color >> 16) & 0xFF) as u8;
        let g = ((color >> 8) & 0xFF) as u8;
        let b = (color & 0xFF) as u8;

        unsafe {
            match self.pixel_fmt {
                PixelFormat::Rgb => {
                    fb.add(offset).write_volatile(r);
                    fb.add(offset + 1).write_volatile(g);
                    fb.add(offset + 2).write_volatile(b);
                }
                PixelFormat::Bgr => {
                    fb.add(offset).write_volatile(b);
                    fb.add(offset + 1).write_volatile(g);
                    fb.add(offset + 2).write_volatile(r);
                }
                _ => {
                    fb.add(offset).write_volatile(r);
                    fb.add(offset + 1).write_volatile(g);
                    fb.add(offset + 2).write_volatile(b);
                }
            }
        }
    }

    fn scroll(&mut self) {
        let row_bytes = self.fb_stride * FONT_HEIGHT * self.fb_bpp;
        let fb = self.fb_addr as *mut u8;
        let total_bytes = self.fb_stride * self.fb_height * self.fb_bpp;
        unsafe {
            core::ptr::copy(fb.add(row_bytes), fb, total_bytes - row_bytes);
            core::ptr::write_bytes(fb.add(total_bytes - row_bytes), 0, row_bytes);
        }
        if self.row > 0 {
            self.row -= 1;
        }
    }
}
