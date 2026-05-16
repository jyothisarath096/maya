#![allow(dead_code)]

pub mod fuzz;
pub mod table;

#[allow(unused_imports)]
pub use table::{
    CapToken, ResourceType, Rights, check_right, check_right_as, create, get_resource_id, revoke,
    validate,
};

pub fn init() {
    table::init();
}
