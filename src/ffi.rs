use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::engine::{CpuEngine, EngineSnapshot};

pub const STATUS_OK: i32 = 0;
pub const STATUS_SAMPLE_READY: i32 = 1;
pub const STATUS_ERROR_INVALID_ARGUMENT: i32 = -1;
pub const STATUS_ERROR_ENGINE: i32 = -2;
pub const STATUS_ERROR_PANIC: i32 = -3;

pub struct EngineHandle(CpuEngine);

/// Creates a per-core CPU engine and writes its opaque handle to `out_engine`.
///
/// # Safety
///
/// `out_engine` must be null or point to writable storage for one engine
/// pointer. A successful handle must eventually be passed exactly once to
/// [`rasitop_engine_destroy`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rasitop_engine_create(out_engine: *mut *mut EngineHandle) -> i32 {
    if out_engine.is_null() {
        return STATUS_ERROR_INVALID_ARGUMENT;
    }

    unsafe {
        out_engine.write(std::ptr::null_mut());
    }

    match catch_unwind(AssertUnwindSafe(|| CpuEngine::new(true))) {
        Ok(Ok(engine)) => {
            let engine = Box::into_raw(Box::new(EngineHandle(engine)));
            unsafe {
                out_engine.write(engine);
            }
            STATUS_OK
        }
        Ok(Err(_)) => STATUS_ERROR_ENGINE,
        Err(_) => STATUS_ERROR_PANIC,
    }
}

/// Samples the engine into caller-owned fixed-layout storage.
///
/// Returns [`STATUS_SAMPLE_READY`] when `out_snapshot` was updated,
/// [`STATUS_OK`] when counters did not advance, or a negative error code.
///
/// # Safety
///
/// `engine` must be a live handle returned by [`rasitop_engine_create`] and
/// accessed by only one thread at a time. `out_snapshot` must point to writable
/// storage for one [`EngineSnapshot`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rasitop_engine_sample(
    engine: *mut EngineHandle,
    out_snapshot: *mut EngineSnapshot,
) -> i32 {
    if engine.is_null() || out_snapshot.is_null() {
        return STATUS_ERROR_INVALID_ARGUMENT;
    }

    match catch_unwind(AssertUnwindSafe(|| {
        let engine = unsafe { &mut *engine };
        engine.0.sample()
    })) {
        Ok(Ok(Some(snapshot))) => {
            unsafe {
                out_snapshot.write(*snapshot);
            }
            STATUS_SAMPLE_READY
        }
        Ok(Ok(None)) => STATUS_OK,
        Ok(Err(_)) => STATUS_ERROR_ENGINE,
        Err(_) => STATUS_ERROR_PANIC,
    }
}

/// Destroys an engine returned by [`rasitop_engine_create`]. Passing null is a
/// successful no-op.
///
/// # Safety
///
/// A non-null `engine` must be a live handle returned by
/// [`rasitop_engine_create`] and must not be used again after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rasitop_engine_destroy(engine: *mut EngineHandle) -> i32 {
    if engine.is_null() {
        return STATUS_OK;
    }

    match catch_unwind(AssertUnwindSafe(|| unsafe {
        drop(Box::from_raw(engine));
    })) {
        Ok(()) => STATUS_OK,
        Err(_) => STATUS_ERROR_PANIC,
    }
}

#[cfg(test)]
mod tests {
    use std::mem::{align_of, offset_of, size_of};

    use super::{
        STATUS_ERROR_INVALID_ARGUMENT, STATUS_OK, rasitop_engine_create, rasitop_engine_destroy,
        rasitop_engine_sample,
    };
    use crate::cpu::{CpuSample, PerCoreSample};
    use crate::engine::EngineSnapshot;

    #[test]
    fn ffi_layout_matches_public_header() {
        assert_eq!(size_of::<CpuSample>(), 40);
        assert_eq!(align_of::<CpuSample>(), 8);
        assert_eq!(offset_of!(CpuSample, user_ratio), 8);
        assert_eq!(size_of::<PerCoreSample>(), 48);
        assert_eq!(align_of::<PerCoreSample>(), 8);
        assert_eq!(offset_of!(PerCoreSample, usage), 8);
        assert_eq!(size_of::<EngineSnapshot>(), 3_152);
        assert_eq!(align_of::<EngineSnapshot>(), 8);
        assert_eq!(offset_of!(EngineSnapshot, aggregate), 32);
        assert_eq!(offset_of!(EngineSnapshot, per_core_count), 72);
        assert_eq!(offset_of!(EngineSnapshot, per_core), 80);
    }

    #[test]
    fn null_arguments_are_rejected_without_dereferencing() {
        assert_eq!(
            unsafe { rasitop_engine_create(std::ptr::null_mut()) },
            STATUS_ERROR_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rasitop_engine_sample(std::ptr::null_mut(), std::ptr::null_mut()) },
            STATUS_ERROR_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rasitop_engine_destroy(std::ptr::null_mut()) },
            STATUS_OK
        );
    }
}
