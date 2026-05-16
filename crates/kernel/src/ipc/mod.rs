#![allow(dead_code)]

pub mod channel;

#[allow(unused_imports)]
pub use channel::{Message, create_channel, recv, send, send_cross_core};

pub fn init() {
    channel::init();
}
