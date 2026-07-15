use std::string::FromUtf8Error;

use thiserror::Error;

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

    #[error("IOServiceOpen returned a null AppleSMC connection")]
    NullConnection,

    #[error("AppleSMC call failed with IOKit status {status}")]
    Call { status: i32 },

    #[error("AppleSMC returned {actual} bytes, expected {expected}")]
    OutputSize { actual: usize, expected: usize },

    #[error("SMC key {key} not found")]
    KeyNotFound { key: String },

    #[error("AppleSMC returned result {result} for key {key}")]
    SmcResult { key: String, result: u8 },

    #[error("SMC key {key} reports an impossible size {actual}; maximum is {maximum}")]
    KeyDataSize {
        key: String,
        actual: u32,
        maximum: usize,
    },

    #[error("#KEY has unexpected size {actual}; expected 4")]
    KeyCountSize { actual: u32 },

    #[error("unsupported SMC data type {data_type:?}")]
    UnsupportedDataType { data_type: String },

    #[error("SMC data type {data_type:?} expects {expected} bytes, got {actual}")]
    DataTypeSize {
        data_type: String,
        expected: usize,
        actual: u32,
    },

    #[error("expected {expected} SMC value bytes, got {actual}")]
    ValueSize { expected: usize, actual: usize },

    #[error("SMC value is not finite")]
    NonFiniteValue,

    #[error("{operation} failed with status {status}")]
    CpuBrandSysctl {
        operation: &'static str,
        status: i32,
    },

    #[error("CPU brand sysctl returned an empty value")]
    EmptyCpuBrand,

    #[error("CPU brand is not UTF-8")]
    CpuBrandUtf8(#[source] FromUtf8Error),

    #[error("unsupported CPU brand {brand:?}")]
    UnsupportedCpu { brand: String },

    #[error("no supported SMC sensors resolved")]
    NoSensors,
}
