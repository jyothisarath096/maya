pub mod canvas;
pub mod driver;
pub mod font;
pub mod virtio_gpu;

pub use driver::{clear, flush_all, flush_region, init, set_pixel};
pub use driver::{FB_HEIGHT, FB_PHYS, FB_SIZE, FB_WIDTH};
pub use canvas::{draw_background, draw_static_layout, render_frame};
pub use font::{draw_char, draw_hex, draw_str, draw_u64, CHAR_H, CHAR_W};
