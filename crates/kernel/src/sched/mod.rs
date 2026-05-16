#![allow(dead_code)]

pub mod balance;
pub mod policy;
pub mod process;
pub mod queue;
pub mod timer;

pub fn init() {
    queue::init();
    policy::init();
    timer::init();
    crate::serial_print("scheduler running\n");
}
