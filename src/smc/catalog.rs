use super::value::SmcKey;

const M1_CPU_TEMPERATURES: &[SmcKey] = &keys([
    *b"Tp09", *b"Tp0T", *b"Tp01", *b"Tp05", *b"Tp0D", *b"Tp0H", *b"Tp0L", *b"Tp0P", *b"Tp0X",
    *b"Tp0b",
]);
const M2_CPU_TEMPERATURES: &[SmcKey] = &keys([
    *b"Tp1h", *b"Tp1t", *b"Tp1p", *b"Tp1l", *b"Tp01", *b"Tp05", *b"Tp09", *b"Tp0D", *b"Tp0X",
    *b"Tp0b", *b"Tp0f", *b"Tp0j",
]);
const M3_CPU_TEMPERATURES: &[SmcKey] = &keys([
    *b"Te05", *b"Te0L", *b"Te0P", *b"Te0S", *b"Tf04", *b"Tf09", *b"Tf0A", *b"Tf0B", *b"Tf0D",
    *b"Tf0E", *b"Tf44", *b"Tf49", *b"Tf4A", *b"Tf4B", *b"Tf4D", *b"Tf4E",
]);
const M4_CPU_TEMPERATURES: &[SmcKey] = &keys([
    *b"Te05", *b"Te0S", *b"Te09", *b"Te0H", *b"Tp01", *b"Tp05", *b"Tp09", *b"Tp0D", *b"Tp0V",
    *b"Tp0Y", *b"Tp0b", *b"Tp0e",
]);
const M5_CPU_TEMPERATURES: &[SmcKey] = &keys([
    *b"Tp00", *b"Tp04", *b"Tp08", *b"Tp0C", *b"Tp0G", *b"Tp0K", *b"Tp0O", *b"Tp0R", *b"Tp0U",
    *b"Tp0X", *b"Tp0a", *b"Tp0d", *b"Tp0g", *b"Tp0j", *b"Tp0m", *b"Tp0p", *b"Tp0u", *b"Tp0y",
]);

const POWER: &[SmcKey] = &keys([*b"PSTR"]);

const fn keys<const N: usize>(values: [[u8; 4]; N]) -> [SmcKey; N] {
    let mut output = [SmcKey::from_bytes(*b"    "); N];
    let mut index = 0;
    while index < N {
        output[index] = SmcKey::from_bytes(values[index]);
        index += 1;
    }
    output
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChipFamily {
    M1,
    M2,
    M3,
    M4,
    M5,
}

impl ChipFamily {
    fn from_brand(brand: &str) -> Option<Self> {
        [
            ("M1", Self::M1),
            ("M2", Self::M2),
            ("M3", Self::M3),
            ("M4", Self::M4),
            ("M5", Self::M5),
        ]
        .into_iter()
        .find_map(|(needle, family)| brand.contains(needle).then_some(family))
    }

    fn temperatures(self) -> &'static [SmcKey] {
        match self {
            Self::M1 => M1_CPU_TEMPERATURES,
            Self::M2 => M2_CPU_TEMPERATURES,
            Self::M3 => M3_CPU_TEMPERATURES,
            Self::M4 => M4_CPU_TEMPERATURES,
            Self::M5 => M5_CPU_TEMPERATURES,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Catalog {
    pub cpu_temperatures: &'static [SmcKey],
    pub power: &'static [SmcKey],
}

pub(super) fn catalog_for_brand(brand: &str) -> Option<Catalog> {
    let family = ChipFamily::from_brand(brand)?;
    Some(Catalog {
        cpu_temperatures: family.temperatures(),
        power: POWER,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SensorRole {
    CpuTemperature,
    FanSpeed,
    SystemPower,
}

impl SensorRole {
    pub fn plausible(self, value: f64) -> bool {
        value.is_finite()
            && match self {
                Self::CpuTemperature => (10.0..=125.0).contains(&value),
                Self::FanSpeed => (0.0..=20_000.0).contains(&value),
                Self::SystemPower => (0.0..=1_000.0).contains(&value),
            }
    }

    pub fn capability(self) -> u64 {
        match self {
            Self::CpuTemperature => super::CAPABILITY_CPU_TEMPERATURE,
            Self::FanSpeed => super::CAPABILITY_FAN_SPEED,
            Self::SystemPower => super::CAPABILITY_SYSTEM_POWER,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ChipFamily, M4_CPU_TEMPERATURES, SensorRole, catalog_for_brand};

    #[test]
    fn selects_bounded_stats_catalog_for_m4() {
        let catalog = catalog_for_brand("Apple M4 Pro").expect("M4 catalog");
        assert_eq!(catalog.cpu_temperatures, M4_CPU_TEMPERATURES);
        assert_eq!(catalog.cpu_temperatures[0].name(), "Te05");
        assert_eq!(catalog.cpu_temperatures.len(), 12);
        assert_eq!(ChipFamily::from_brand("Unknown"), None);
    }

    #[test]
    fn role_validation_keeps_idle_fans_but_rejects_zero_temperature() {
        assert!(SensorRole::FanSpeed.plausible(0.0));
        assert!(SensorRole::SystemPower.plausible(0.0));
        assert!(!SensorRole::CpuTemperature.plausible(0.0));
        assert!(SensorRole::CpuTemperature.plausible(42.0));
        assert!(!SensorRole::FanSpeed.plausible(20_001.0));
    }
}
