use thiserror::Error;

use super::{ERROR_SMC_ACCESS, ERROR_SMC_DATA, ERROR_SMC_IO};

pub type Result<T> = std::result::Result<T, SmcError>;

#[derive(Debug, Error)]
pub enum SmcError {
    #[error("AppleSMC access requires macOS")]
    UnsupportedPlatform,

    #[error("IOServiceMatching(AppleSMC) failed")]
    ServiceMatching,

    #[error("AppleSMC service not found")]
    ServiceNotFound,

    #[error("IOServiceOpen(AppleSMC) failed with IOKit status {status}")]
    Open { status: i32 },

    #[error("AppleSMC call failed with IOKit status {status}")]
    Call { status: i32 },

    #[error("SMC key {key} not found")]
    KeyNotFound { key: String },

    #[error("AppleSMC returned result {result} for key {key}")]
    SmcResult { key: String, result: u8 },

    #[error("unsupported SMC data type {data_type:?}")]
    UnsupportedDataType { data_type: String },

    #[error("SMC value is not finite")]
    NonFiniteValue,

    #[error("{operation} failed with status {status}")]
    CpuBrandSysctl {
        operation: &'static str,
        status: i32,
    },

    #[error("unsupported CPU brand {brand:?}")]
    UnsupportedCpu { brand: String },

    #[error("no supported SMC sensors resolved")]
    NoSensors,
}

impl SmcError {
    /// Broad failure category for snapshots, CSV, and the C ABI.
    pub const fn category_flag(&self) -> u64 {
        match self {
            Self::UnsupportedPlatform
            | Self::ServiceMatching
            | Self::ServiceNotFound
            | Self::Open { .. }
            | Self::CpuBrandSysctl { .. }
            | Self::UnsupportedCpu { .. } => ERROR_SMC_ACCESS,
            Self::Call { .. } | Self::KeyNotFound { .. } | Self::SmcResult { .. } => ERROR_SMC_IO,
            Self::UnsupportedDataType { .. } | Self::NonFiniteValue | Self::NoSensors => {
                ERROR_SMC_DATA
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ERROR_SMC_ACCESS, ERROR_SMC_DATA, ERROR_SMC_IO, SmcError};

    #[test]
    fn errors_map_to_broad_runtime_categories() {
        assert_eq!(
            SmcError::Open { status: 1 }.category_flag(),
            ERROR_SMC_ACCESS
        );
        assert_eq!(
            SmcError::KeyNotFound { key: "PSTR".into() }.category_flag(),
            ERROR_SMC_IO
        );
        assert_eq!(SmcError::NonFiniteValue.category_flag(), ERROR_SMC_DATA);
    }
}
