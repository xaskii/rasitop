//! Diagnostics-only validation of an exact, evidence-backed GPU layout.

use std::collections::BTreeMap;
use std::ffi::{c_char, c_void};
use std::io::{Read, Write};
use std::time::Duration;

use serde::Serialize;
use thiserror::Error;

use crate::ioreport::{self, ResidencyRecord, ResidencySelector};

pub const GPU_SCHEMA_VERSION: u32 = 1;

const M4_PRO_STATES: [&str; 16] = [
    "OFF", "P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8", "P9", "P10", "P11", "P12", "P13", "P14",
    "P15",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuCatalogEntry {
    pub machine_model: &'static str,
    pub os_build: &'static str,
    pub group: &'static str,
    pub subgroup: &'static str,
    pub channel: &'static str,
    pub unit: &'static str,
    pub states: &'static [&'static str],
    pub idle_state_index: usize,
}

pub const M4_PRO_26A5388G: GpuCatalogEntry = GpuCatalogEntry {
    machine_model: "Mac16,8",
    os_build: "26A5388g",
    group: "GPU Stats",
    subgroup: "GPU Performance States",
    channel: "GPUPH",
    unit: "24Mticks",
    states: &M4_PRO_STATES,
    idle_state_index: 0,
};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GpuDiagnosticSample {
    pub schema_version: u32,
    pub sequence: u64,
    pub monotonic_ms: u64,
    pub sample_duration_us: u64,
    pub residency_total_ticks: u64,
    pub residency_idle_ticks: u64,
    pub residency_busy_ticks: u64,
    pub gpu_busy_ratio: f64,
}

#[derive(Debug, Error)]
pub enum GpuError {
    #[error(
        "unsupported GPU telemetry layout for machine {machine_model:?}, OS build {os_build:?}"
    )]
    UnsupportedLayout {
        machine_model: String,
        os_build: String,
    },
    #[error("read system identity {name}: errno {errno}")]
    SystemIdentity { name: &'static str, errno: i32 },
    #[error("system identity {name} is not valid UTF-8")]
    InvalidSystemIdentity { name: &'static str },
    #[error("IOReport diagnostics: {0}")]
    IoReport(#[from] ioreport::IoReportError),
    #[error("sequence {sequence} contains unexpected channel metadata")]
    UnexpectedChannel { sequence: u64 },
    #[error("sequence {sequence} contains state index {index}, outside the catalog")]
    StateIndexOutOfRange { sequence: u64, index: u32 },
    #[error("sequence {sequence} state {index} is {actual:?}; expected {expected:?}")]
    StateNameMismatch {
        sequence: u64,
        index: u32,
        expected: &'static str,
        actual: String,
    },
    #[error("sequence {sequence} contains duplicate state index {index}")]
    DuplicateState { sequence: u64, index: u32 },
    #[error("sequence {sequence} is missing one or more catalog states")]
    MissingState { sequence: u64 },
    #[error("sequence {sequence} has inconsistent or overflowing residency totals")]
    InvalidTotal { sequence: u64 },
    #[error("write GPU diagnostics: {0}")]
    Csv(#[from] csv::Error),
    #[error("flush GPU diagnostics: {0}")]
    Io(#[from] std::io::Error),
}

pub fn catalog_for(machine_model: &str, os_build: &str) -> Option<&'static GpuCatalogEntry> {
    (machine_model == M4_PRO_26A5388G.machine_model && os_build == M4_PRO_26A5388G.os_build)
        .then_some(&M4_PRO_26A5388G)
}

pub fn current_catalog() -> Result<&'static GpuCatalogEntry, GpuError> {
    let machine_model = system_identity(c"hw.model", "hw.model")?;
    let os_build = system_identity(c"kern.osversion", "kern.osversion")?;
    catalog_for(&machine_model, &os_build).ok_or(GpuError::UnsupportedLayout {
        machine_model,
        os_build,
    })
}

pub fn capture_validated(
    interval: Duration,
    count: u64,
) -> Result<Vec<GpuDiagnosticSample>, GpuError> {
    let catalog = current_catalog()?;
    let records = ioreport::capture_residencies(
        &ResidencySelector {
            group: catalog.group.into(),
            subgroup: catalog.subgroup.into(),
            channel: catalog.channel.into(),
        },
        interval,
        count,
    )?;
    decode(catalog, &records)
}

pub fn decode(
    catalog: &'static GpuCatalogEntry,
    records: &[ResidencyRecord],
) -> Result<Vec<GpuDiagnosticSample>, GpuError> {
    let mut sequences: BTreeMap<u64, Vec<&ResidencyRecord>> = BTreeMap::new();
    for record in records {
        sequences.entry(record.sequence).or_default().push(record);
    }
    sequences
        .into_iter()
        .map(|(sequence, records)| decode_sequence(catalog, sequence, &records))
        .collect()
}

fn decode_sequence(
    catalog: &'static GpuCatalogEntry,
    sequence: u64,
    records: &[&ResidencyRecord],
) -> Result<GpuDiagnosticSample, GpuError> {
    let mut ticks = vec![None; catalog.states.len()];
    let mut declared_total = None;
    for record in records {
        if record.group != catalog.group
            || record.subgroup != catalog.subgroup
            || record.channel != catalog.channel
            || record.unit != catalog.unit
        {
            return Err(GpuError::UnexpectedChannel { sequence });
        }
        let index = record.state_index as usize;
        let expected = catalog
            .states
            .get(index)
            .ok_or(GpuError::StateIndexOutOfRange {
                sequence,
                index: record.state_index,
            })?;
        if record.state_name != *expected {
            return Err(GpuError::StateNameMismatch {
                sequence,
                index: record.state_index,
                expected,
                actual: record.state_name.clone(),
            });
        }
        if ticks[index].replace(record.residency_ticks).is_some() {
            return Err(GpuError::DuplicateState {
                sequence,
                index: record.state_index,
            });
        }
        match declared_total {
            Some(total) if total != record.total_ticks => {
                return Err(GpuError::InvalidTotal { sequence });
            }
            None => declared_total = Some(record.total_ticks),
            _ => {}
        }
    }
    if ticks.iter().any(Option::is_none) {
        return Err(GpuError::MissingState { sequence });
    }
    let total = ticks.iter().try_fold(0_u64, |total, value| {
        total.checked_add(value.expect("checked above"))
    });
    let total = total
        .filter(|total| Some(*total) == declared_total && *total != 0)
        .ok_or(GpuError::InvalidTotal { sequence })?;
    let idle = ticks[catalog.idle_state_index].expect("checked above");
    let busy = total - idle;
    let first = records.first().ok_or(GpuError::MissingState { sequence })?;
    Ok(GpuDiagnosticSample {
        schema_version: GPU_SCHEMA_VERSION,
        sequence,
        monotonic_ms: first.monotonic_ms,
        sample_duration_us: first.sample_duration_us,
        residency_total_ticks: total,
        residency_idle_ticks: idle,
        residency_busy_ticks: busy,
        gpu_busy_ratio: busy as f64 / total as f64,
    })
}

pub fn write_csv<W: Write>(writer: W, samples: &[GpuDiagnosticSample]) -> Result<(), GpuError> {
    let mut csv = csv::WriterBuilder::new()
        .has_headers(true)
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(writer);
    for sample in samples {
        csv.serialize(sample)?;
    }
    csv.flush()?;
    Ok(())
}

pub fn decode_csv<R: Read>(reader: R) -> Result<Vec<GpuDiagnosticSample>, GpuError> {
    let catalog = current_catalog()?;
    let records = csv::Reader::from_reader(reader)
        .deserialize()
        .collect::<Result<Vec<ResidencyRecord>, _>>()?;
    decode(catalog, &records)
}

fn system_identity(
    name: &'static std::ffi::CStr,
    display_name: &'static str,
) -> Result<String, GpuError> {
    let mut length = 0_usize;
    let status = unsafe {
        sysctlbyname(
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return Err(GpuError::SystemIdentity {
            name: display_name,
            errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
        });
    }
    let mut bytes = vec![0_u8; length];
    let status = unsafe {
        sysctlbyname(
            name.as_ptr(),
            bytes.as_mut_ptr().cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return Err(GpuError::SystemIdentity {
            name: display_name,
            errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
        });
    }
    if bytes.last() == Some(&0) {
        bytes.pop();
    }
    String::from_utf8(bytes).map_err(|_| GpuError::InvalidSystemIdentity { name: display_name })
}

unsafe extern "C" {
    fn sysctlbyname(
        name: *const c_char,
        old_value: *mut c_void,
        old_length: *mut usize,
        new_value: *mut c_void,
        new_length: usize,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records() -> Vec<ResidencyRecord> {
        M4_PRO_STATES
            .iter()
            .enumerate()
            .map(|(index, name)| ResidencyRecord {
                sequence: 7,
                monotonic_ms: 2_000,
                sample_duration_us: 400,
                group: "GPU Stats".into(),
                subgroup: "GPU Performance States".into(),
                channel: "GPUPH".into(),
                unit: "24Mticks".into(),
                state_index: index as u32,
                state_name: (*name).into(),
                residency_ticks: if index == 0 {
                    75
                } else {
                    u64::from(index == 1) * 25
                },
                total_ticks: 100,
                state_ratio: None,
            })
            .collect()
    }

    #[test]
    fn exact_layout_decodes_busy_residency_independent_of_row_order() {
        let mut fixture = records();
        fixture.reverse();
        let sample = decode(&M4_PRO_26A5388G, &fixture).expect("validated fixture");
        assert_eq!(sample[0].residency_busy_ticks, 25);
        assert_eq!(sample[0].gpu_busy_ratio, 0.25);
    }

    #[test]
    fn unknown_or_reordered_state_names_fail_closed() {
        let mut fixture = records();
        fixture[1].state_name = "P2".into();
        assert!(matches!(
            decode(&M4_PRO_26A5388G, &fixture),
            Err(GpuError::StateNameMismatch { index: 1, .. })
        ));
    }

    #[test]
    fn missing_states_and_zero_totals_fail_closed() {
        let mut missing = records();
        missing.pop();
        assert!(matches!(
            decode(&M4_PRO_26A5388G, &missing),
            Err(GpuError::MissingState { .. })
        ));

        let mut zero = records();
        for record in &mut zero {
            record.residency_ticks = 0;
            record.total_ticks = 0;
        }
        assert!(matches!(
            decode(&M4_PRO_26A5388G, &zero),
            Err(GpuError::InvalidTotal { .. })
        ));
    }

    #[test]
    fn catalog_is_exact_to_machine_and_os_build() {
        assert!(catalog_for("Mac16,8", "26A5388g").is_some());
        assert!(catalog_for("Mac16,8", "future").is_none());
        assert!(catalog_for("Mac16,7", "26A5388g").is_none());
    }
}
