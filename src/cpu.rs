use thiserror::Error;

type Result<T> = std::result::Result<T, CpuError>;

#[derive(Debug, Error)]
pub enum CpuError {
    #[error("host_statistics(HOST_CPU_LOAD_INFO) failed with kern_return_t {status}")]
    HostStatistics { status: i32 },

    #[error("host_processor_info(PROCESSOR_CPU_LOAD_INFO) failed with kern_return_t {status}")]
    HostProcessorInfo { status: i32 },
}

const CPU_STATE_USER: usize = 0;
const CPU_STATE_SYSTEM: usize = 1;
const CPU_STATE_IDLE: usize = 2;
const CPU_STATE_NICE: usize = 3;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CpuTicks {
    user: u32,
    system: u32,
    idle: u32,
    nice: u32,
}

impl CpuTicks {
    fn deltas_since(self, previous: Self) -> CpuDeltas {
        CpuDeltas {
            user: self.user.wrapping_sub(previous.user) as u64,
            system: self.system.wrapping_sub(previous.system) as u64,
            idle: self.idle.wrapping_sub(previous.idle) as u64,
            nice: self.nice.wrapping_sub(previous.nice) as u64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CpuDeltas {
    user: u64,
    system: u64,
    idle: u64,
    nice: u64,
}

impl CpuDeltas {
    fn ratios(self) -> Option<CpuSample> {
        let total = self.user + self.system + self.idle + self.nice;
        if total == 0 {
            return None;
        }

        let ratio = |ticks| ticks as f64 / total as f64;
        let user_ratio = ratio(self.user);
        let system_ratio = ratio(self.system);
        let idle_ratio = ratio(self.idle);
        let nice_ratio = ratio(self.nice);

        Some(CpuSample {
            total_ratio: user_ratio + system_ratio + nice_ratio,
            user_ratio,
            system_ratio,
            nice_ratio,
            idle_ratio,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CpuSample {
    pub total_ratio: f64,
    pub user_ratio: f64,
    pub system_ratio: f64,
    pub nice_ratio: f64,
    pub idle_ratio: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PerCoreSample {
    pub logical_cpu: u32,
    pub usage: CpuSample,
}

#[derive(Debug, Default)]
pub struct MachCpuProvider {
    previous: Option<CpuTicks>,
}

impl MachCpuProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads the cumulative host counters and returns utilization since the
    /// previous read. The first call establishes the baseline and returns
    /// `None`.
    pub fn sample(&mut self) -> Result<Option<CpuSample>> {
        let current = read_cpu_ticks()?;
        let previous = self.previous.replace(current);
        Ok(previous.and_then(|previous| current.deltas_since(previous).ratios()))
    }

    /// Discards the previous counters and establishes a fresh baseline.
    pub fn reset(&mut self) -> Result<()> {
        self.previous = None;
        self.sample().map(|_| ())
    }
}

#[derive(Debug, Default)]
pub struct MachPerCoreProvider {
    previous: Vec<CpuTicks>,
    samples: Vec<PerCoreSample>,
}

impl MachPerCoreProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads per-logical-CPU counters. Returns `true` when `samples()` contains
    /// utilization since the previous read. A first observation or topology
    /// change establishes a new baseline and returns `false`.
    pub fn sample(&mut self) -> Result<bool> {
        let current = read_per_core_ticks()?;
        let current = current.as_slice();

        if self.previous.len() != current.len() {
            self.previous.clear();
            self.previous.extend_from_slice(current);
            self.samples.clear();
            self.samples.reserve(current.len());
            return Ok(false);
        }

        build_per_core_samples(current, &self.previous, &mut self.samples);
        self.previous.copy_from_slice(current);
        Ok(true)
    }

    pub fn samples(&self) -> &[PerCoreSample] {
        &self.samples
    }

    /// Discards the previous counters and establishes a fresh baseline.
    pub fn reset(&mut self) -> Result<()> {
        self.previous.clear();
        self.samples.clear();
        self.sample().map(|_| ())
    }
}

fn build_per_core_samples(
    current: &[CpuTicks],
    previous: &[CpuTicks],
    samples: &mut Vec<PerCoreSample>,
) {
    debug_assert_eq!(current.len(), previous.len());
    samples.clear();

    for (logical_cpu, (&current, &previous)) in current.iter().zip(previous).enumerate() {
        let Some(usage) = current.deltas_since(previous).ratios() else {
            continue;
        };
        let logical_cpu = u32::try_from(logical_cpu).expect("logical CPU index must fit in u32");
        samples.push(PerCoreSample { logical_cpu, usage });
    }
}

fn read_cpu_ticks() -> Result<CpuTicks> {
    use std::mem::{MaybeUninit, size_of};
    use std::os::raw::c_int;

    type KernReturn = c_int;
    type MachPort = u32;
    type MachMsgTypeNumber = u32;
    type HostFlavor = c_int;

    const KERN_SUCCESS: KernReturn = 0;
    const HOST_CPU_LOAD_INFO: HostFlavor = 3;

    #[repr(C)]
    struct HostCpuLoadInfo {
        cpu_ticks: [u32; 4],
    }

    unsafe extern "C" {
        fn mach_host_self() -> MachPort;
        fn host_statistics(
            host_priv: MachPort,
            flavor: HostFlavor,
            host_info_out: *mut c_int,
            host_info_out_count: *mut MachMsgTypeNumber,
        ) -> KernReturn;
    }

    let mut info = MaybeUninit::<HostCpuLoadInfo>::uninit();
    let expected_count = size_of::<HostCpuLoadInfo>() / size_of::<c_int>();
    let mut count = MachMsgTypeNumber::try_from(expected_count)
        .expect("HOST_CPU_LOAD_INFO count must fit in mach_msg_type_number_t");

    // SAFETY: `info` points to writable storage with the layout required by
    // HOST_CPU_LOAD_INFO, and `count` reports that storage in integer_t units.
    // On success, Mach initializes exactly the reported fields before return.
    let status = unsafe {
        host_statistics(
            mach_host_self(),
            HOST_CPU_LOAD_INFO,
            info.as_mut_ptr().cast::<c_int>(),
            &mut count,
        )
    };

    if status != KERN_SUCCESS {
        return Err(CpuError::HostStatistics { status });
    }
    assert!(
        count as usize >= expected_count,
        "host_statistics returned too few CPU counters"
    );

    // SAFETY: a successful `host_statistics` call with the expected count
    // initialized the complete `HostCpuLoadInfo` value.
    let ticks = unsafe { info.assume_init() }.cpu_ticks;
    Ok(CpuTicks {
        user: ticks[CPU_STATE_USER],
        system: ticks[CPU_STATE_SYSTEM],
        idle: ticks[CPU_STATE_IDLE],
        nice: ticks[CPU_STATE_NICE],
    })
}

struct ProcessorInfoBuffer {
    pointer: *mut std::os::raw::c_int,
    processor_count: usize,
    integer_count: usize,
}

impl ProcessorInfoBuffer {
    fn as_slice(&self) -> &[CpuTicks] {
        // SAFETY: `host_processor_info` returned `processor_count` complete
        // PROCESSOR_CPU_LOAD_INFO records. `CpuTicks` has the same four-u32
        // field layout, and the buffer remains live for `self`'s lifetime.
        unsafe { std::slice::from_raw_parts(self.pointer.cast::<CpuTicks>(), self.processor_count) }
    }
}

impl Drop for ProcessorInfoBuffer {
    fn drop(&mut self) {
        use std::mem::size_of;

        unsafe extern "C" {
            static mach_task_self_: u32;
            fn vm_deallocate(target_task: u32, address: usize, size: usize) -> i32;
        }

        let byte_count = self.integer_count * size_of::<std::os::raw::c_int>();
        // SAFETY: this is the exact address and byte length allocated by
        // `host_processor_info` in the current task. This guard drops once.
        let _ = unsafe { vm_deallocate(mach_task_self_, self.pointer as usize, byte_count) };
    }
}

fn read_per_core_ticks() -> Result<ProcessorInfoBuffer> {
    use std::mem::size_of;
    use std::os::raw::c_int;
    use std::ptr;

    const KERN_SUCCESS: c_int = 0;
    const PROCESSOR_CPU_LOAD_INFO: c_int = 2;
    const INTEGERS_PER_CPU: usize = size_of::<CpuTicks>() / size_of::<c_int>();

    unsafe extern "C" {
        fn mach_host_self() -> u32;
        fn host_processor_info(
            host: u32,
            flavor: c_int,
            out_processor_count: *mut u32,
            out_processor_info: *mut *mut c_int,
            out_processor_info_count: *mut u32,
        ) -> c_int;
    }

    let mut processor_count = 0_u32;
    let mut pointer = ptr::null_mut();
    let mut integer_count = 0_u32;

    // SAFETY: all out-parameters point to valid writable storage. On success,
    // Mach returns a VM-allocated integer array owned by this task; the guard
    // below releases it with `vm_deallocate`.
    let status = unsafe {
        host_processor_info(
            mach_host_self(),
            PROCESSOR_CPU_LOAD_INFO,
            &mut processor_count,
            &mut pointer,
            &mut integer_count,
        )
    };
    if status != KERN_SUCCESS {
        return Err(CpuError::HostProcessorInfo { status });
    }
    assert!(!pointer.is_null(), "host_processor_info returned null");

    let buffer = ProcessorInfoBuffer {
        pointer,
        processor_count: processor_count as usize,
        integer_count: integer_count as usize,
    };
    let expected_count = buffer
        .processor_count
        .checked_mul(INTEGERS_PER_CPU)
        .expect("per-core CPU counter count overflowed");
    assert_eq!(
        buffer.integer_count, expected_count,
        "host_processor_info returned a malformed CPU buffer"
    );

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::{CpuDeltas, CpuError, CpuTicks, PerCoreSample, build_per_core_samples};

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }

    #[test]
    fn mach_failures_preserve_the_native_status() {
        let error = CpuError::HostStatistics { status: 5 };
        assert_eq!(
            error.to_string(),
            "host_statistics(HOST_CPU_LOAD_INFO) failed with kern_return_t 5"
        );
    }

    #[test]
    fn computes_cpu_ratios_from_tick_deltas() {
        let previous = CpuTicks {
            user: 100,
            system: 200,
            idle: 300,
            nice: 400,
        };
        let current = CpuTicks {
            user: 140,
            system: 220,
            idle: 330,
            nice: 410,
        };

        let sample = current
            .deltas_since(previous)
            .ratios()
            .expect("non-zero delta");

        assert_close(sample.user_ratio, 0.4);
        assert_close(sample.system_ratio, 0.2);
        assert_close(sample.idle_ratio, 0.3);
        assert_close(sample.nice_ratio, 0.1);
        assert_close(sample.total_ratio, 0.7);
    }

    #[test]
    fn returns_none_when_no_ticks_advance() {
        assert_eq!(
            CpuDeltas {
                user: 0,
                system: 0,
                idle: 0,
                nice: 0,
            }
            .ratios(),
            None
        );
    }

    #[test]
    fn handles_natural_t_counter_wrap() {
        let previous = CpuTicks {
            user: u32::MAX - 1,
            system: 10,
            idle: 20,
            nice: 30,
        };
        let current = CpuTicks {
            user: 2,
            system: 11,
            idle: 22,
            nice: 33,
        };

        assert_eq!(
            current.deltas_since(previous),
            CpuDeltas {
                user: 4,
                system: 1,
                idle: 2,
                nice: 3,
            }
        );
    }

    #[test]
    fn preserves_user_and_system_ratios_for_each_logical_cpu() {
        let previous = [
            CpuTicks {
                user: 10,
                system: 20,
                idle: 30,
                nice: 40,
            },
            CpuTicks {
                user: 100,
                system: 200,
                idle: 300,
                nice: 400,
            },
        ];
        let current = [
            CpuTicks {
                user: 14,
                system: 22,
                idle: 33,
                nice: 41,
            },
            CpuTicks {
                user: 101,
                system: 205,
                idle: 302,
                nice: 402,
            },
        ];
        let mut samples = Vec::<PerCoreSample>::new();

        build_per_core_samples(&current, &previous, &mut samples);

        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].logical_cpu, 0);
        assert_close(samples[0].usage.user_ratio, 0.4);
        assert_close(samples[0].usage.system_ratio, 0.2);
        assert_close(samples[0].usage.total_ratio, 0.7);
        assert_eq!(samples[1].logical_cpu, 1);
        assert_close(samples[1].usage.user_ratio, 0.1);
        assert_close(samples[1].usage.system_ratio, 0.5);
        assert_close(samples[1].usage.total_ratio, 0.8);
    }
}
