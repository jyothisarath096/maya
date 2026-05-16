#![allow(dead_code)]

pub mod channel;
pub mod fuzz;

pub fn init() {
    channel::init();
}
