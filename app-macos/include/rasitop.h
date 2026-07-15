#ifndef CPU_MONITOR_FFI_H
#define CPU_MONITOR_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define rasitop_max_logical_cpus 64

#define rasitop_ok 0
#define rasitop_sample_ready 1
#define rasitop_error_invalid_argument -1
#define rasitop_error_engine -2
#define rasitop_error_panic -3

#define rasitop_sensor_capability_cpu_temperature UINT64_C(0x1)
#define rasitop_sensor_capability_fan_speed UINT64_C(0x2)
#define rasitop_sensor_capability_system_power UINT64_C(0x4)

#define rasitop_sensor_error_cpu_temperature UINT64_C(0x1)
#define rasitop_sensor_error_fan_speed UINT64_C(0x2)
#define rasitop_sensor_error_system_power UINT64_C(0x4)
#define rasitop_sensor_error_smc_initialization UINT64_C(0x8000000000000000)
#define rasitop_sensor_error_smc_access UINT64_C(0x4000000000000000)
#define rasitop_sensor_error_smc_io UINT64_C(0x2000000000000000)
#define rasitop_sensor_error_smc_data UINT64_C(0x1000000000000000)

typedef struct rasitop_engine rasitop_engine;

typedef struct {
  double total_ratio;
  double user_ratio;
  double system_ratio;
  double nice_ratio;
  double idle_ratio;
} rasitop_cpu_sample;

typedef struct {
  uint32_t logical_cpu;
  rasitop_cpu_sample usage;
} rasitop_per_core_sample;

typedef struct {
  double cpu_temp_max_c;
  double cpu_temp_avg_c;
  double fan_rpm;
  double system_power_w;
  uint64_t capability_flags;
  uint64_t error_flags;
} rasitop_sensor_sample;

typedef struct {
  uint64_t sequence;
  uint64_t monotonic_ns;
  uint64_t interval_ns;
  uint64_t sample_duration_ns;
  rasitop_cpu_sample aggregate;
  uint32_t per_core_count;
  rasitop_per_core_sample per_core[rasitop_max_logical_cpus];
  rasitop_sensor_sample sensors;
} rasitop_engine_snapshot;

int32_t rasitop_engine_create(rasitop_engine **out_engine);
int32_t rasitop_engine_sample(rasitop_engine *engine,
                              rasitop_engine_snapshot *out_snapshot);
int32_t rasitop_engine_destroy(rasitop_engine *engine);

static inline const rasitop_per_core_sample *
rasitop_snapshot_core(const rasitop_engine_snapshot *snapshot, uint32_t index) {
  if (snapshot == 0 || index >= snapshot->per_core_count ||
      index >= rasitop_max_logical_cpus) {
    return 0;
  }
  return &snapshot->per_core[index];
}

_Static_assert(sizeof(rasitop_cpu_sample) == 40,
               "rasitop_cpu_sample ABI layout changed");
_Static_assert(offsetof(rasitop_cpu_sample, user_ratio) == 8,
               "rasitop_cpu_sample ABI field order changed");
_Static_assert(sizeof(rasitop_per_core_sample) == 48,
               "rasitop_per_core_sample ABI layout changed");
_Static_assert(offsetof(rasitop_per_core_sample, usage) == 8,
               "rasitop_per_core_sample ABI field order changed");
_Static_assert(sizeof(rasitop_sensor_sample) == 48,
               "rasitop_sensor_sample ABI layout changed");
_Static_assert(offsetof(rasitop_sensor_sample, fan_rpm) == 16,
               "rasitop_sensor_sample field order changed");
_Static_assert(offsetof(rasitop_sensor_sample, system_power_w) == 24,
               "rasitop_sensor_sample field order changed");
_Static_assert(offsetof(rasitop_sensor_sample, capability_flags) == 32,
               "rasitop_sensor_sample field order changed");
_Static_assert(sizeof(rasitop_engine_snapshot) == 3200,
               "rasitop_engine_snapshot ABI layout changed");
_Static_assert(offsetof(rasitop_engine_snapshot, aggregate) == 32,
               "rasitop_engine_snapshot aggregate offset changed");
_Static_assert(offsetof(rasitop_engine_snapshot, per_core_count) == 72,
               "rasitop_engine_snapshot core count offset changed");
_Static_assert(offsetof(rasitop_engine_snapshot, per_core) == 80,
               "rasitop_engine_snapshot core array offset changed");
_Static_assert(offsetof(rasitop_engine_snapshot, sensors) == 3152,
               "rasitop_engine_snapshot sensor offset changed");

#ifdef __cplusplus
}
#endif

#endif
