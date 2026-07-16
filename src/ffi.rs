use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::engine::{CpuEngine, EngineSnapshot, HistoryPoint, SampleRequest};

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

/// Samples the engine into caller-owned fixed-layout storage. Aggregate CPU is
/// always sampled; `request_flags` controls the optional per-core and sensor
/// reads. Fields that are not requested retain their cached values, except that
/// `per_core_count` is zero when per-core sampling was not requested.
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
    request_flags: u32,
    out_snapshot: *mut EngineSnapshot,
) -> i32 {
    if engine.is_null() || out_snapshot.is_null() {
        return STATUS_ERROR_INVALID_ARGUMENT;
    }
    let Some(request) = SampleRequest::from_bits(request_flags) else {
        return STATUS_ERROR_INVALID_ARGUMENT;
    };

    match catch_unwind(AssertUnwindSafe(|| {
        let engine = unsafe { &mut *engine };
        engine.0.sample(request)
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

/// Re-establishes aggregate and per-core CPU counter baselines after a pause.
///
/// # Safety
///
/// `engine` must be a live handle returned by [`rasitop_engine_create`] and
/// accessed by only one thread at a time.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rasitop_engine_reset_cpu_baselines(engine: *mut EngineHandle) -> i32 {
    if engine.is_null() {
        return STATUS_ERROR_INVALID_ARGUMENT;
    }

    match catch_unwind(AssertUnwindSafe(|| {
        let engine = unsafe { &mut *engine };
        engine.0.reset_cpu_baselines()
    })) {
        Ok(Ok(())) => STATUS_OK,
        Ok(Err(_)) => STATUS_ERROR_ENGINE,
        Err(_) => STATUS_ERROR_PANIC,
    }
}

/// Copies the latest aggregate CPU history into caller-owned storage in
/// oldest-to-newest order and returns the number of points written.
///
/// # Safety
///
/// `engine` must be a live handle returned by [`rasitop_engine_create`] and
/// accessed by only one thread at a time. When `capacity` is non-zero,
/// `out_points` must point to writable storage for that many [`HistoryPoint`]
/// values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rasitop_engine_history(
    engine: *mut EngineHandle,
    out_points: *mut HistoryPoint,
    capacity: usize,
) -> usize {
    if engine.is_null() || (out_points.is_null() && capacity != 0) {
        return 0;
    }

    catch_unwind(AssertUnwindSafe(|| {
        let engine = unsafe { &*engine };
        if capacity == 0 {
            0
        } else {
            let output = unsafe { std::slice::from_raw_parts_mut(out_points, capacity) };
            engine.0.history(output)
        }
    }))
    .unwrap_or(0)
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
        rasitop_engine_history, rasitop_engine_reset_cpu_baselines, rasitop_engine_sample,
    };
    use crate::cpu::{CpuSample, PerCoreSample};
    use crate::engine::{EngineSnapshot, HistoryPoint};
    use crate::smc::SensorSample;

    #[test]
    fn ffi_layout_matches_public_header() {
        assert_eq!(size_of::<CpuSample>(), 40);
        assert_eq!(align_of::<CpuSample>(), 8);
        assert_eq!(offset_of!(CpuSample, user_ratio), 8);
        assert_eq!(size_of::<PerCoreSample>(), 48);
        assert_eq!(align_of::<PerCoreSample>(), 8);
        assert_eq!(offset_of!(PerCoreSample, usage), 8);
        assert_eq!(size_of::<SensorSample>(), 48);
        assert_eq!(align_of::<SensorSample>(), 8);
        assert_eq!(offset_of!(SensorSample, fan_rpm), 16);
        assert_eq!(offset_of!(SensorSample, system_power_w), 24);
        assert_eq!(offset_of!(SensorSample, capability_flags), 32);
        assert_eq!(size_of::<EngineSnapshot>(), 3_200);
        assert_eq!(align_of::<EngineSnapshot>(), 8);
        assert_eq!(offset_of!(EngineSnapshot, aggregate), 32);
        assert_eq!(offset_of!(EngineSnapshot, per_core_count), 72);
        assert_eq!(offset_of!(EngineSnapshot, per_core), 80);
        assert_eq!(offset_of!(EngineSnapshot, sensors), 3_152);
        assert_eq!(size_of::<HistoryPoint>(), 16);
        assert_eq!(align_of::<HistoryPoint>(), 8);
        assert_eq!(offset_of!(HistoryPoint, total_ratio), 8);
    }

    #[test]
    fn null_arguments_are_rejected_without_dereferencing() {
        assert_eq!(
            unsafe { rasitop_engine_create(std::ptr::null_mut()) },
            STATUS_ERROR_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rasitop_engine_sample(std::ptr::null_mut(), 0, std::ptr::null_mut()) },
            STATUS_ERROR_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rasitop_engine_reset_cpu_baselines(std::ptr::null_mut()) },
            STATUS_ERROR_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rasitop_engine_history(std::ptr::null_mut(), std::ptr::null_mut(), 0) },
            0
        );
        assert_eq!(
            unsafe { rasitop_engine_destroy(std::ptr::null_mut()) },
            STATUS_OK
        );
    }

    #[cfg(all(target_os = "macos", not(miri)))]
    #[test]
    fn live_engine_cpu_baselines_can_be_reset() {
        let mut engine = std::ptr::null_mut();

        assert_eq!(unsafe { rasitop_engine_create(&mut engine) }, STATUS_OK);
        assert!(!engine.is_null());
        assert_eq!(
            unsafe { rasitop_engine_reset_cpu_baselines(engine) },
            STATUS_OK
        );
        assert_eq!(unsafe { rasitop_engine_destroy(engine) }, STATUS_OK);
    }
}
