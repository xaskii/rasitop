pub mod activity;
pub mod cpu;
pub mod engine;
pub mod ffi;
pub mod ioreport;
pub mod measure;
pub mod record;
pub mod smc;

#[cfg(all(test, target_os = "macos"))]
mod test_allocator;
