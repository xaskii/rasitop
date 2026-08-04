#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
compile_error!("rasitop supports Apple silicon Macs only");

pub mod cpu;
pub mod engine;
pub mod ffi;
pub mod gpu;
pub mod ioreport;
pub mod measure;
pub mod record;
pub mod smc;

#[cfg(all(any(test, feature = "gpu-profiling"), target_os = "macos"))]
pub mod test_allocator;
