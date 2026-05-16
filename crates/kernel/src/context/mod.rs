#![allow(dead_code)]

pub mod store;

#[allow(unused_imports)]
pub use store::{delete, expire, get, init, set, snapshot};
