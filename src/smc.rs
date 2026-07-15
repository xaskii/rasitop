//! Typed, read-only AppleSMC sensor access.
//!
//! Normal sampling resolves a bounded, Stats-compatible catalog once and then
//! issues value reads only. Full key enumeration is confined to `discover`.

mod catalog;
mod discovery;
mod error;
mod protocol;
mod value;

use catalog::{SensorRole, catalog_for_brand};
pub use discovery::{DiagnosticKey, DiagnosticReport, discover};
use error::Result;
pub use error::SmcError;
use protocol::SmcConnection;
use value::{ResolvedKey, SmcKey};

pub const CAPABILITY_CPU_TEMPERATURE: u64 = 1 << 0;
pub const CAPABILITY_FAN_SPEED: u64 = 1 << 1;
pub const CAPABILITY_SYSTEM_POWER: u64 = 1 << 2;

pub const ERROR_CPU_TEMPERATURE: u64 = 1 << 0;
pub const ERROR_FAN_SPEED: u64 = 1 << 1;
pub const ERROR_SYSTEM_POWER: u64 = 1 << 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SensorSample {
    pub cpu_temp_max_c: f64,
    pub cpu_temp_avg_c: f64,
    pub fan_rpm: f64,
    pub system_power_w: f64,
    pub capability_flags: u64,
    pub error_flags: u64,
}

impl Default for SensorSample {
    fn default() -> Self {
        Self {
            cpu_temp_max_c: f64::NAN,
            cpu_temp_avg_c: f64::NAN,
            fan_rpm: f64::NAN,
            system_power_w: f64::NAN,
            capability_flags: 0,
            error_flags: 0,
        }
    }
}

impl SensorSample {
    pub fn cpu_temp_max_c(&self) -> Option<f64> {
        available(
            self.cpu_temp_max_c,
            self.capability_flags,
            CAPABILITY_CPU_TEMPERATURE,
        )
    }

    pub fn cpu_temp_avg_c(&self) -> Option<f64> {
        available(
            self.cpu_temp_avg_c,
            self.capability_flags,
            CAPABILITY_CPU_TEMPERATURE,
        )
    }

    pub fn fan_rpm(&self) -> Option<f64> {
        available(self.fan_rpm, self.capability_flags, CAPABILITY_FAN_SPEED)
    }

    pub fn system_power_w(&self) -> Option<f64> {
        available(
            self.system_power_w,
            self.capability_flags,
            CAPABILITY_SYSTEM_POWER,
        )
    }
}

fn available(value: f64, capabilities: u64, capability: u64) -> Option<f64> {
    ((capabilities & capability) != 0 && value.is_finite()).then_some(value)
}

#[derive(Debug)]
struct Reader {
    role: SensorRole,
    keys: Vec<ResolvedKey>,
}

impl Reader {
    fn resolve(connection: &SmcConnection, role: SensorRole, candidates: &[value::SmcKey]) -> Self {
        let keys = candidates
            .iter()
            .filter_map(|&key| ResolvedKey::resolve(connection, key).ok())
            .filter(|key| {
                key.read(connection)
                    .is_ok_and(|value| role.plausible(value))
            })
            .collect();
        Self { role, keys }
    }

    fn capability(&self) -> u64 {
        if self.keys.is_empty() {
            0
        } else {
            self.role.capability()
        }
    }

    fn read(&self, connection: &SmcConnection) -> Reading {
        let mut maximum = f64::NEG_INFINITY;
        let mut sum = 0.0;
        let mut count = 0_u32;
        let mut had_error = false;

        for key in &self.keys {
            match key.read(connection) {
                Ok(value) if self.role.plausible(value) => {
                    maximum = maximum.max(value);
                    sum += value;
                    count += 1;
                }
                Ok(_) | Err(_) => had_error = true,
            }
        }

        Reading {
            maximum: (count != 0).then_some(maximum),
            average: (count != 0).then_some(sum / f64::from(count)),
            had_error: had_error || (!self.keys.is_empty() && count == 0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Reading {
    maximum: Option<f64>,
    average: Option<f64>,
    had_error: bool,
}

#[derive(Debug)]
pub struct SmcProvider {
    connection: SmcConnection,
    temperatures: Reader,
    fans: Reader,
    power: Reader,
    capability_flags: u64,
}

impl SmcProvider {
    pub fn new() -> Result<Self> {
        let brand = protocol::cpu_brand()?;
        let catalog = catalog_for_brand(&brand).ok_or_else(|| SmcError::UnsupportedCpu {
            brand: brand.clone(),
        })?;
        let connection = SmcConnection::open()?;
        let temperatures = Reader::resolve(
            &connection,
            SensorRole::CpuTemperature,
            catalog.cpu_temperatures,
        );
        let fan_candidates = fan_candidates(&connection);
        let fans = Reader::resolve(&connection, SensorRole::FanSpeed, &fan_candidates);
        let power = Reader::resolve(&connection, SensorRole::SystemPower, catalog.power);
        let capability_flags = temperatures.capability() | fans.capability() | power.capability();
        if capability_flags == 0 {
            return Err(SmcError::NoSensors);
        }

        Ok(Self {
            connection,
            temperatures,
            fans,
            power,
            capability_flags,
        })
    }

    /// Reads only values for keys resolved at initialization. Metadata and the
    /// full key catalog are never queried on this hot path.
    pub fn sample(&mut self) -> SensorSample {
        let temperatures = self.temperatures.read(&self.connection);
        let fans = self.fans.read(&self.connection);
        let power = self.power.read(&self.connection);
        let mut error_flags = 0;
        if temperatures.had_error {
            error_flags |= ERROR_CPU_TEMPERATURE;
        }
        if fans.had_error {
            error_flags |= ERROR_FAN_SPEED;
        }
        if power.had_error {
            error_flags |= ERROR_SYSTEM_POWER;
        }

        SensorSample {
            cpu_temp_max_c: temperatures.maximum.unwrap_or(f64::NAN),
            cpu_temp_avg_c: temperatures.average.unwrap_or(f64::NAN),
            // A machine may have multiple fans. The maximum represents active
            // cooling without hiding one spinning fan behind an average.
            fan_rpm: fans.maximum.unwrap_or(f64::NAN),
            system_power_w: power.maximum.unwrap_or(f64::NAN),
            capability_flags: self.capability_flags,
            error_flags,
        }
    }
}

fn fan_candidates(connection: &SmcConnection) -> Vec<SmcKey> {
    let count = ResolvedKey::resolve(connection, SmcKey::from_bytes(*b"FNum"))
        .and_then(|key| key.read(connection))
        .ok()
        .filter(|count| count.fract() == 0.0 && (0.0..=10.0).contains(count))
        .map_or(0, |count| count as u8);

    (0..count)
        .map(|index| SmcKey::from_bytes([b'F', b'0' + index, b'A', b'c']))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{CAPABILITY_CPU_TEMPERATURE, CAPABILITY_FAN_SPEED, SensorSample, available};

    #[test]
    fn accessors_require_capability_and_a_finite_value() {
        let sample = SensorSample {
            cpu_temp_max_c: 62.5,
            cpu_temp_avg_c: f64::NAN,
            fan_rpm: 0.0,
            system_power_w: 7.25,
            capability_flags: CAPABILITY_CPU_TEMPERATURE | CAPABILITY_FAN_SPEED,
            error_flags: 0,
        };
        assert_eq!(sample.cpu_temp_max_c(), Some(62.5));
        assert_eq!(sample.cpu_temp_avg_c(), None);
        assert_eq!(sample.fan_rpm(), Some(0.0));
        assert_eq!(sample.system_power_w(), None);
        assert_eq!(available(f64::INFINITY, u64::MAX, 1), None);
    }
}
