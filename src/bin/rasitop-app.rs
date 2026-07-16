use rasitop::ffi::{
    rasitop_engine_create, rasitop_engine_destroy, rasitop_engine_reset_cpu_baselines,
    rasitop_engine_sample,
};

unsafe extern "C" {
    fn rasitop_app_main();
}

fn main() {
    // Keep the C ABI exports reachable from the Swift static library.
    std::hint::black_box([
        rasitop_engine_create as *const () as usize,
        rasitop_engine_sample as *const () as usize,
        rasitop_engine_reset_cpu_baselines as *const () as usize,
        rasitop_engine_destroy as *const () as usize,
    ]);

    unsafe {
        rasitop_app_main();
    }
}
