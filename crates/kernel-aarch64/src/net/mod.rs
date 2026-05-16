pub mod driver;
pub mod udp;
pub mod virtio;
pub mod virtqueue;

pub fn init() {
    if driver::init() {
        crate::uart_print!("NET: virtio-net ready\n");
        crate::fs::namespace::mkdir(b"/sys/net").ok();
    } else {
        crate::uart_print!("NET: init failed\n");
    }
}

