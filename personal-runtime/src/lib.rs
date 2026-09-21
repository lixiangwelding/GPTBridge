//! Local, single-user cooperative runtime. No network server or hidden agent loop.
pub mod locks;
pub mod store;
pub mod tasks;
mod task_open;
pub mod jobs;
pub mod worker;
pub mod config;

pub use store::{Store, Error, Result, digest, now_ms};
