pub mod keyboard;

pub fn init() {
    if keyboard::init() {
        crate::uart_print!("INPUT: keyboard ready\n");
    } else {
        crate::uart_print!("INPUT: keyboard unavailable\n");
    }
}
