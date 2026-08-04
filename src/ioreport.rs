//! Diagnostics-only access to the private IOReport catalog and raw samples.
//!
//! Subscription ownership remains private. A normal rasitop launch cannot
//! touch IOReport; only explicit `gpu` diagnostic commands call this module.

use std::ffi::{c_char, c_void};
use std::io::Write;
use std::ptr::NonNull;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

type CfIndex = isize;
type CfTypeRef = *const c_void;
type CfArrayRef = *const c_void;
type CfDictionaryRef = *const c_void;
type CfStringRef = *const c_void;

const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const CHANNELS_KEY: &[u8] = b"IOReportChannels\0";
const CHANNEL_INFO_KEY: &[u8] = b"IOReportChannelInfo\0";
const STATE_NAMES_KEY: &[u8] = b"IOReportChannelStateNames\0";

#[derive(Debug, Error)]
pub enum IoReportError {
    #[error("IOReport returned no channel dictionary")]
    MissingChannelDictionary,
    #[error("IOReport channel dictionary has unexpected Core Foundation type")]
    InvalidChannelDictionary,
    #[error("IOReport channel list is missing or has unexpected Core Foundation type")]
    InvalidChannelList,
    #[error("IOReport channel at index {index} has unexpected Core Foundation type")]
    InvalidChannel { index: usize },
    #[error("IOReport returned an invalid state count {count} for channel {channel:?}")]
    InvalidStateCount { channel: String, count: i32 },
    #[error("IOReport could not copy channel group {group:?} subgroup {subgroup:?}")]
    MissingChannelGroup { group: String, subgroup: String },
    #[error("IOReport could not create a subscription")]
    SubscriptionFailed,
    #[error("IOReport could not create a sample")]
    SampleFailed,
    #[error("IOReport could not create a sample delta")]
    DeltaFailed,
    #[error("IOReport sample did not contain {group:?}/{subgroup:?}/{channel:?}")]
    MissingSampleChannel {
        group: String,
        subgroup: String,
        channel: String,
    },
    #[error("residency for state {index} of channel {channel:?} was negative: {value}")]
    NegativeResidency {
        channel: String,
        index: u32,
        value: i64,
    },
    #[error("residency total overflowed for channel {channel:?}")]
    ResidencyOverflow { channel: String },
    #[error("Core Foundation string is too large to decode")]
    StringTooLarge,
    #[error("Core Foundation could not decode a string as UTF-8")]
    InvalidString,
    #[error("write channel inventory: {0}")]
    Csv(#[from] csv::Error),
    #[error("flush channel inventory: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChannelRecord {
    pub group: String,
    pub subgroup: String,
    pub channel: String,
    pub unit: String,
    pub kind: String,
    pub state_index: Option<u32>,
    pub state_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResidencySelector {
    pub group: String,
    pub subgroup: String,
    pub channel: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ResidencyRecord {
    pub sequence: u64,
    pub monotonic_ms: u64,
    pub sample_duration_us: u64,
    pub group: String,
    pub subgroup: String,
    pub channel: String,
    pub unit: String,
    pub state_index: u32,
    pub state_name: String,
    pub residency_ticks: u64,
    pub total_ticks: u64,
    pub state_ratio: Option<f64>,
}

pub fn discover() -> Result<Vec<ChannelRecord>, IoReportError> {
    platform::discover()
}

pub fn write_csv<W: Write>(writer: W, records: &[ChannelRecord]) -> Result<(), IoReportError> {
    let mut csv = csv::WriterBuilder::new()
        .has_headers(true)
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(writer);
    for record in records {
        csv.serialize(record)?;
    }
    csv.flush()?;
    Ok(())
}

pub fn record_residencies<W: Write>(
    writer: W,
    selector: &ResidencySelector,
    interval: Duration,
    count: u64,
) -> Result<(), IoReportError> {
    let records = capture_residencies(selector, interval, count)?;
    write_residency_csv(writer, &records)
}

pub fn capture_residencies(
    selector: &ResidencySelector,
    interval: Duration,
    count: u64,
) -> Result<Vec<ResidencyRecord>, IoReportError> {
    platform::capture_residencies(selector, interval, count)
}

pub fn write_residency_csv<W: Write>(
    writer: W,
    records: &[ResidencyRecord],
) -> Result<(), IoReportError> {
    let mut csv = csv::WriterBuilder::new()
        .has_headers(true)
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(writer);
    for record in records {
        csv.serialize(record)?;
    }
    csv.flush()?;
    Ok(())
}

fn kind_name(format: u8) -> String {
    match format {
        0 => "invalid".into(),
        1 => "simple".into(),
        2 => "state".into(),
        3 => "histogram".into(),
        4 => "simple_array".into(),
        unknown => format!("unknown:{unknown}"),
    }
}

fn records_for_channel(
    group: String,
    subgroup: String,
    channel: String,
    unit: String,
    format: u8,
    states: Vec<String>,
) -> Vec<ChannelRecord> {
    let kind = kind_name(format);
    if format == 2 && !states.is_empty() {
        states
            .into_iter()
            .enumerate()
            .map(|(index, state_name)| ChannelRecord {
                group: group.clone(),
                subgroup: subgroup.clone(),
                channel: channel.clone(),
                unit: unit.clone(),
                kind: kind.clone(),
                state_index: Some(index as u32),
                state_name,
            })
            .collect()
    } else {
        vec![ChannelRecord {
            group,
            subgroup,
            channel,
            unit,
            kind,
            state_index: None,
            state_name: String::new(),
        }]
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod platform {
    use std::time::Instant;

    use super::*;

    type ReleaseFn = unsafe fn(CfTypeRef);

    struct OwnedCf {
        pointer: NonNull<c_void>,
        release: ReleaseFn,
    }

    impl OwnedCf {
        unsafe fn from_copy(pointer: CfTypeRef) -> Option<Self> {
            NonNull::new(pointer.cast_mut()).map(|pointer| Self {
                pointer,
                release: cf_release,
            })
        }

        fn as_ptr(&self) -> CfTypeRef {
            self.pointer.as_ptr()
        }
    }

    impl Drop for OwnedCf {
        fn drop(&mut self) {
            // SAFETY: `pointer` is a non-null object returned at +1 and this
            // owner calls its matching release function exactly once.
            unsafe { (self.release)(self.as_ptr()) }
        }
    }

    pub(super) fn discover() -> Result<Vec<ChannelRecord>, IoReportError> {
        // SAFETY: The private function has no pointer inputs. A non-null result
        // follows Core Foundation's Copy rule and is immediately owned.
        let catalog = unsafe { OwnedCf::from_copy(io_report_copy_all_channels(0, 0)) }
            .ok_or(IoReportError::MissingChannelDictionary)?;
        checked_type(catalog.as_ptr(), unsafe { cf_dictionary_get_type_id() })
            .ok_or(IoReportError::InvalidChannelDictionary)?;

        let channels = dictionary_value(catalog.as_ptr(), CHANNELS_KEY)
            .ok_or(IoReportError::InvalidChannelList)?;
        checked_type(channels, unsafe { cf_array_get_type_id() })
            .ok_or(IoReportError::InvalidChannelList)?;

        // SAFETY: `channels` is a checked CFArray retained by `catalog`.
        let count = unsafe { cf_array_get_count(channels) };
        let mut records = Vec::new();
        for index in 0..count {
            // SAFETY: `index` is within the checked array's bounds.
            let item = unsafe { cf_array_get_value_at_index(channels, index) };
            checked_type(item, unsafe { cf_dictionary_get_type_id() }).ok_or(
                IoReportError::InvalidChannel {
                    index: index as usize,
                },
            )?;
            records.extend(decode_channel(item)?);
        }
        records.sort_by(|left, right| {
            (
                &left.group,
                &left.subgroup,
                &left.channel,
                &left.unit,
                &left.kind,
                left.state_index,
                &left.state_name,
            )
                .cmp(&(
                    &right.group,
                    &right.subgroup,
                    &right.channel,
                    &right.unit,
                    &right.kind,
                    right.state_index,
                    &right.state_name,
                ))
        });
        Ok(records)
    }

    pub(super) fn capture_residencies(
        selector: &ResidencySelector,
        interval: Duration,
        count: u64,
    ) -> Result<Vec<ResidencyRecord>, IoReportError> {
        let mut session = SamplingSession::new(selector)?;
        let started = Instant::now();
        let mut output = Vec::new();
        for sequence in 0..count {
            std::thread::sleep(interval);
            let sample_started = Instant::now();
            output.extend(session.sample(sequence, started.elapsed(), sample_started)?);
        }
        Ok(output)
    }

    struct SamplingSession<'a> {
        selector: &'a ResidencySelector,
        baseline: OwnedCf,
        subscription: OwnedCf,
        subscribed_channels: Option<OwnedCf>,
        channels: OwnedCf,
    }

    impl<'a> SamplingSession<'a> {
        fn new(selector: &'a ResidencySelector) -> Result<Self, IoReportError> {
            // Discovery deliberately subscribes to the copied inventory and
            // filters only after sampling. Narrow subscriptions belong to the
            // later provider revision, once this capture validates a layout.
            let copied_channels = unsafe { OwnedCf::from_copy(io_report_copy_all_channels(0, 0)) }
                .ok_or_else(|| IoReportError::MissingChannelGroup {
                    group: selector.group.clone(),
                    subgroup: selector.subgroup.clone(),
                })?;
            checked_type(copied_channels.as_ptr(), unsafe {
                cf_dictionary_get_type_id()
            })
            .ok_or(IoReportError::InvalidChannelDictionary)?;
            // IOReportCreateSubscription mutates its channel selection.
            let channels = unsafe {
                OwnedCf::from_copy(cf_dictionary_create_mutable_copy(
                    std::ptr::null(),
                    cf_dictionary_get_count(copied_channels.as_ptr()),
                    copied_channels.as_ptr(),
                ))
            }
            .ok_or(IoReportError::InvalidChannelDictionary)?;
            retain_only_channel(channels.as_ptr(), selector)?;

            let mut subscribed_channels = std::ptr::null();
            // SAFETY: `channels` is a live copied channel dictionary and the
            // out-pointer is initialized. A non-null return is a +1 object.
            let subscription = unsafe {
                OwnedCf::from_copy(io_report_create_subscription(
                    std::ptr::null(),
                    channels.as_ptr(),
                    &mut subscribed_channels,
                    0,
                    std::ptr::null(),
                ))
            }
            .ok_or(IoReportError::SubscriptionFailed)?;
            // IOReport writes a separate +1 channel dictionary on systems that
            // need to adjust the selection. Keep it alive for the subscription.
            let subscribed_channels = unsafe { OwnedCf::from_copy(subscribed_channels) };
            let sample_channels = subscribed_channels
                .as_ref()
                .map_or(channels.as_ptr(), OwnedCf::as_ptr);
            // SAFETY: Subscription and its channel dictionary remain live.
            let baseline = unsafe {
                OwnedCf::from_copy(io_report_create_samples(
                    subscription.as_ptr(),
                    sample_channels,
                    std::ptr::null(),
                ))
            }
            .ok_or(IoReportError::SampleFailed)?;
            Ok(Self {
                selector,
                baseline,
                subscription,
                subscribed_channels,
                channels,
            })
        }

        fn sample(
            &mut self,
            sequence: u64,
            elapsed: Duration,
            sample_started: Instant,
        ) -> Result<Vec<ResidencyRecord>, IoReportError> {
            let sample_channels = self
                .subscribed_channels
                .as_ref()
                .map_or(self.channels.as_ptr(), OwnedCf::as_ptr);
            // SAFETY: The subscription and channel dictionary are owned by self.
            let next = unsafe {
                OwnedCf::from_copy(io_report_create_samples(
                    self.subscription.as_ptr(),
                    sample_channels,
                    std::ptr::null(),
                ))
            }
            .ok_or(IoReportError::SampleFailed)?;
            // SAFETY: Both sample dictionaries are live for this call.
            let delta = unsafe {
                OwnedCf::from_copy(io_report_create_samples_delta(
                    self.baseline.as_ptr(),
                    next.as_ptr(),
                    std::ptr::null(),
                ))
            }
            .ok_or(IoReportError::DeltaFailed)?;
            self.baseline = next;
            decode_residency_delta(
                delta.as_ptr(),
                self.selector,
                sequence,
                elapsed,
                sample_started.elapsed(),
            )
        }
    }

    fn decode_residency_delta(
        delta: CfDictionaryRef,
        selector: &ResidencySelector,
        sequence: u64,
        elapsed: Duration,
        sample_duration: Duration,
    ) -> Result<Vec<ResidencyRecord>, IoReportError> {
        let channels = dictionary_value(delta, CHANNELS_KEY)
            .and_then(|value| checked_type(value, unsafe { cf_array_get_type_id() }))
            .ok_or(IoReportError::InvalidChannelList)?;
        let count = unsafe { cf_array_get_count(channels) };
        for item_index in 0..count {
            let item = unsafe { cf_array_get_value_at_index(channels, item_index) };
            if checked_type(item, unsafe { cf_dictionary_get_type_id() }).is_none() {
                return Err(IoReportError::InvalidChannel {
                    index: item_index as usize,
                });
            }
            let group = string_or_empty(unsafe { io_report_channel_get_group(item) })?;
            let subgroup = string_or_empty(unsafe { io_report_channel_get_sub_group(item) })?;
            let channel = string_or_empty(unsafe { io_report_channel_get_channel_name(item) })?;
            if group != selector.group
                || subgroup != selector.subgroup
                || channel != selector.channel
            {
                continue;
            }
            let unit = string_or_empty(unsafe { io_report_channel_get_unit_label(item) })?;
            let state_count = unsafe { io_report_state_get_count(item) };
            if state_count < 0 {
                return Err(IoReportError::InvalidStateCount {
                    channel,
                    count: state_count,
                });
            }
            let mut states = Vec::with_capacity(state_count as usize);
            let mut total_ticks = 0_u64;
            for state_index in 0..state_count {
                let value = unsafe { io_report_state_get_residency(item, state_index) };
                let residency_ticks =
                    u64::try_from(value).map_err(|_| IoReportError::NegativeResidency {
                        channel: channel.clone(),
                        index: state_index as u32,
                        value,
                    })?;
                total_ticks = total_ticks.checked_add(residency_ticks).ok_or_else(|| {
                    IoReportError::ResidencyOverflow {
                        channel: channel.clone(),
                    }
                })?;
                states.push((
                    state_index as u32,
                    string_or_empty(unsafe {
                        io_report_state_get_name_for_index(item, state_index)
                    })?,
                    residency_ticks,
                ));
            }
            return Ok(states
                .into_iter()
                .map(
                    |(state_index, state_name, residency_ticks)| ResidencyRecord {
                        sequence,
                        monotonic_ms: elapsed.as_millis().try_into().unwrap_or(u64::MAX),
                        sample_duration_us: sample_duration
                            .as_micros()
                            .try_into()
                            .unwrap_or(u64::MAX),
                        group: group.clone(),
                        subgroup: subgroup.clone(),
                        channel: channel.clone(),
                        unit: unit.clone(),
                        state_index,
                        state_name,
                        residency_ticks,
                        total_ticks,
                        state_ratio: (total_ticks != 0)
                            .then_some(residency_ticks as f64 / total_ticks as f64),
                    },
                )
                .collect());
        }
        Err(IoReportError::MissingSampleChannel {
            group: selector.group.clone(),
            subgroup: selector.subgroup.clone(),
            channel: selector.channel.clone(),
        })
    }

    fn retain_only_channel(
        channels: CfDictionaryRef,
        selector: &ResidencySelector,
    ) -> Result<(), IoReportError> {
        let source = dictionary_value(channels, CHANNELS_KEY)
            .and_then(|value| checked_type(value, unsafe { cf_array_get_type_id() }))
            .ok_or(IoReportError::InvalidChannelList)?;
        let selected = unsafe {
            OwnedCf::from_copy(cf_array_create_mutable_copy(std::ptr::null(), 0, source))
        }
        .ok_or(IoReportError::InvalidChannelList)?;
        let count = unsafe { cf_array_get_count(selected.as_ptr()) };
        for index in (0..count).rev() {
            let item = unsafe { cf_array_get_value_at_index(selected.as_ptr(), index) };
            let matches = checked_type(item, unsafe { cf_dictionary_get_type_id() }).is_some()
                && string_or_empty(unsafe { io_report_channel_get_group(item) })? == selector.group
                && string_or_empty(unsafe { io_report_channel_get_sub_group(item) })?
                    == selector.subgroup
                && string_or_empty(unsafe { io_report_channel_get_channel_name(item) })?
                    == selector.channel;
            if !matches {
                // SAFETY: `selected` is mutable and index is in bounds.
                unsafe { cf_array_remove_value_at_index(selected.as_ptr(), index) };
            }
        }
        if unsafe { cf_array_get_count(selected.as_ptr()) } != 1 {
            return Err(IoReportError::MissingSampleChannel {
                group: selector.group.clone(),
                subgroup: selector.subgroup.clone(),
                channel: selector.channel.clone(),
            });
        }
        let key = create_key(CHANNELS_KEY).ok_or(IoReportError::InvalidChannelList)?;
        // SAFETY: The mutable dictionary and key/value CF objects are live;
        // the dictionary retains the replacement array.
        unsafe { cf_dictionary_set_value(channels, key.as_ptr(), selected.as_ptr()) };
        Ok(())
    }

    fn decode_channel(item: CfDictionaryRef) -> Result<Vec<ChannelRecord>, IoReportError> {
        let group = string_or_empty(unsafe { io_report_channel_get_group(item) })?;
        let subgroup = string_or_empty(unsafe { io_report_channel_get_sub_group(item) })?;
        let channel = string_or_empty(unsafe { io_report_channel_get_channel_name(item) })?;
        let unit = string_or_empty(unsafe { io_report_channel_get_unit_label(item) })?;
        // SAFETY: IOReport accessors receive a checked channel dictionary.
        let format = unsafe { io_report_channel_get_format(item) };
        let mut states = Vec::new();
        if format == 2 {
            // SAFETY: State access is restricted to state-format channels.
            let count = unsafe { io_report_state_get_count(item) };
            if count < 0 {
                return Err(IoReportError::InvalidStateCount { channel, count });
            }
            let channel_info = dictionary_value(item, CHANNEL_INFO_KEY)
                .and_then(|value| checked_type(value, unsafe { cf_dictionary_get_type_id() }));
            let declared_names = channel_info
                .and_then(|info| dictionary_value(info, STATE_NAMES_KEY))
                .and_then(|value| checked_type(value, unsafe { cf_array_get_type_id() }));
            states.reserve(count as usize);
            for index in 0..count {
                let array_index = index as CfIndex;
                let name = if let Some(names) = declared_names {
                    // A shorter optional names array is malformed metadata, but
                    // discovery preserves the state index with an empty name.
                    if array_index < unsafe { cf_array_get_count(names) } {
                        // SAFETY: The checked index is inside the names array.
                        unsafe { cf_array_get_value_at_index(names, array_index) }
                    } else {
                        std::ptr::null()
                    }
                } else {
                    // SAFETY: Index is within the count returned for this channel.
                    unsafe { io_report_state_get_name_for_index(item, index) }
                };
                states.push(string_or_empty(name)?);
            }
        }
        Ok(records_for_channel(
            group, subgroup, channel, unit, format, states,
        ))
    }

    fn checked_type(value: CfTypeRef, expected: usize) -> Option<CfTypeRef> {
        if value.is_null() {
            return None;
        }
        // SAFETY: Core Foundation accepts any non-null CF object here.
        (unsafe { cf_get_type_id(value) } == expected).then_some(value)
    }

    fn dictionary_value(
        dictionary: CfDictionaryRef,
        nul_terminated_key: &[u8],
    ) -> Option<CfTypeRef> {
        // SAFETY: Callers pass static, NUL-terminated UTF-8 key bytes.
        let key = create_key(nul_terminated_key)?;
        // SAFETY: Both arguments are checked CF objects alive for this call.
        let value = unsafe { cf_dictionary_get_value(dictionary, key.as_ptr()) };
        (!value.is_null()).then_some(value)
    }

    fn create_key(nul_terminated_key: &[u8]) -> Option<OwnedCf> {
        let key = unsafe {
            cf_string_create_with_c_string(
                std::ptr::null(),
                nul_terminated_key.as_ptr().cast(),
                CF_STRING_ENCODING_UTF8,
            )
        };
        // SAFETY: `key` was created by a Create function and is either null or
        // transferred to the exactly-once owner below.
        unsafe { OwnedCf::from_copy(key) }
    }

    fn string_or_empty(value: CfStringRef) -> Result<String, IoReportError> {
        let Some(value) = checked_type(value, unsafe { cf_string_get_type_id() }) else {
            return Ok(String::new());
        };
        // SAFETY: `value` is a checked CFString borrowed from the catalog.
        let length = unsafe { cf_string_get_length(value) };
        let capacity =
            unsafe { cf_string_get_maximum_size_for_encoding(length, CF_STRING_ENCODING_UTF8) }
                .checked_add(1)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or(IoReportError::StringTooLarge)?;
        let mut bytes = vec![0_u8; capacity];
        // SAFETY: The buffer is writable for `capacity` bytes and the checked
        // conversion above guarantees it fits in `CfIndex`.
        let converted = unsafe {
            cf_string_get_c_string(
                value,
                bytes.as_mut_ptr().cast(),
                capacity as CfIndex,
                CF_STRING_ENCODING_UTF8,
            )
        };
        if converted == 0 {
            return Err(IoReportError::InvalidString);
        }
        let length = bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(bytes.len());
        String::from_utf8(bytes[..length].to_vec()).map_err(|_| IoReportError::InvalidString)
    }

    unsafe fn cf_release(value: CfTypeRef) {
        unsafe { cf_release_raw(value) }
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        #[link_name = "CFRelease"]
        fn cf_release_raw(value: CfTypeRef);
        fn CFGetTypeID(value: CfTypeRef) -> usize;
        fn CFArrayGetTypeID() -> usize;
        fn CFArrayGetCount(array: CfArrayRef) -> CfIndex;
        fn CFArrayGetValueAtIndex(array: CfArrayRef, index: CfIndex) -> CfTypeRef;
        fn CFArrayCreateMutableCopy(
            allocator: CfTypeRef,
            capacity: CfIndex,
            array: CfArrayRef,
        ) -> CfArrayRef;
        fn CFArrayRemoveValueAtIndex(array: CfArrayRef, index: CfIndex);
        fn CFDictionaryCreateMutableCopy(
            allocator: CfTypeRef,
            capacity: CfIndex,
            dictionary: CfDictionaryRef,
        ) -> CfDictionaryRef;
        fn CFDictionaryGetCount(dictionary: CfDictionaryRef) -> CfIndex;
        fn CFDictionaryGetTypeID() -> usize;
        fn CFDictionaryGetValue(dictionary: CfDictionaryRef, key: CfTypeRef) -> CfTypeRef;
        fn CFDictionarySetValue(dictionary: CfDictionaryRef, key: CfTypeRef, value: CfTypeRef);
        fn CFStringGetTypeID() -> usize;
        fn CFStringCreateWithCString(
            allocator: CfTypeRef,
            string: *const c_char,
            encoding: u32,
        ) -> CfStringRef;
        fn CFStringGetLength(string: CfStringRef) -> CfIndex;
        fn CFStringGetMaximumSizeForEncoding(length: CfIndex, encoding: u32) -> CfIndex;
        fn CFStringGetCString(
            string: CfStringRef,
            buffer: *mut c_char,
            buffer_size: CfIndex,
            encoding: u32,
        ) -> u8;
    }

    #[link(name = "IOReport", kind = "dylib")]
    unsafe extern "C" {
        fn IOReportCopyAllChannels(first: u64, second: u64) -> CfDictionaryRef;
        fn IOReportCreateSubscription(
            service: CfTypeRef,
            channels: CfDictionaryRef,
            subscribed_channels: *mut CfDictionaryRef,
            options: u64,
            error: CfTypeRef,
        ) -> CfTypeRef;
        fn IOReportCreateSamples(
            subscription: CfTypeRef,
            channels: CfDictionaryRef,
            error: CfTypeRef,
        ) -> CfDictionaryRef;
        fn IOReportCreateSamplesDelta(
            previous: CfDictionaryRef,
            current: CfDictionaryRef,
            error: CfTypeRef,
        ) -> CfDictionaryRef;
        fn IOReportChannelGetGroup(channel: CfDictionaryRef) -> CfStringRef;
        fn IOReportChannelGetSubGroup(channel: CfDictionaryRef) -> CfStringRef;
        fn IOReportChannelGetChannelName(channel: CfDictionaryRef) -> CfStringRef;
        fn IOReportChannelGetUnitLabel(channel: CfDictionaryRef) -> CfStringRef;
        fn IOReportChannelGetFormat(channel: CfDictionaryRef) -> u8;
        fn IOReportStateGetCount(channel: CfDictionaryRef) -> i32;
        fn IOReportStateGetNameForIndex(channel: CfDictionaryRef, index: i32) -> CfStringRef;
        fn IOReportStateGetResidency(channel: CfDictionaryRef, index: i32) -> i64;
    }

    use self::{
        CFArrayCreateMutableCopy as cf_array_create_mutable_copy,
        CFArrayGetCount as cf_array_get_count, CFArrayGetTypeID as cf_array_get_type_id,
        CFArrayGetValueAtIndex as cf_array_get_value_at_index,
        CFArrayRemoveValueAtIndex as cf_array_remove_value_at_index,
        CFDictionaryCreateMutableCopy as cf_dictionary_create_mutable_copy,
        CFDictionaryGetCount as cf_dictionary_get_count,
        CFDictionaryGetTypeID as cf_dictionary_get_type_id,
        CFDictionaryGetValue as cf_dictionary_get_value,
        CFDictionarySetValue as cf_dictionary_set_value, CFGetTypeID as cf_get_type_id,
        CFStringCreateWithCString as cf_string_create_with_c_string,
        CFStringGetCString as cf_string_get_c_string, CFStringGetLength as cf_string_get_length,
        CFStringGetMaximumSizeForEncoding as cf_string_get_maximum_size_for_encoding,
        CFStringGetTypeID as cf_string_get_type_id,
        IOReportChannelGetChannelName as io_report_channel_get_channel_name,
        IOReportChannelGetFormat as io_report_channel_get_format,
        IOReportChannelGetGroup as io_report_channel_get_group,
        IOReportChannelGetSubGroup as io_report_channel_get_sub_group,
        IOReportChannelGetUnitLabel as io_report_channel_get_unit_label,
        IOReportCopyAllChannels as io_report_copy_all_channels,
        IOReportCreateSamples as io_report_create_samples,
        IOReportCreateSamplesDelta as io_report_create_samples_delta,
        IOReportCreateSubscription as io_report_create_subscription,
        IOReportStateGetCount as io_report_state_get_count,
        IOReportStateGetNameForIndex as io_report_state_get_name_for_index,
        IOReportStateGetResidency as io_report_state_get_residency,
    };

    #[cfg(test)]
    mod tests {
        use std::sync::atomic::{AtomicUsize, Ordering};

        use super::*;

        static RELEASES: AtomicUsize = AtomicUsize::new(0);

        unsafe fn count_release(_: CfTypeRef) {
            RELEASES.fetch_add(1, Ordering::Relaxed);
        }

        #[test]
        fn copied_object_is_released_exactly_once() {
            RELEASES.store(0, Ordering::Relaxed);
            let pointer =
                NonNull::new(std::ptr::dangling_mut::<c_void>()).expect("non-null test pointer");
            drop(OwnedCf {
                pointer,
                release: count_release,
            });
            assert_eq!(RELEASES.load(Ordering::Relaxed), 1);
        }

        #[test]
        fn null_copy_does_not_create_an_owner() {
            assert!(unsafe { OwnedCf::from_copy(std::ptr::null()) }.is_none());
        }
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
mod platform {
    use super::*;

    pub(super) fn discover() -> Result<Vec<ChannelRecord>, IoReportError> {
        Err(IoReportError::MissingChannelDictionary)
    }

    pub(super) fn capture_residencies(
        _selector: &ResidencySelector,
        _interval: Duration,
        _count: u64,
    ) -> Result<Vec<ResidencyRecord>, IoReportError> {
        Err(IoReportError::MissingChannelDictionary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_channels_expand_to_stable_indexed_rows() {
        let rows = records_for_channel(
            "GPU Stats".into(),
            "GPU Performance States".into(),
            "GPU 0".into(),
            "ticks".into(),
            2,
            vec!["IDLE".into(), "ACTIVE, HIGH".into()],
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].state_index, Some(0));
        assert_eq!(rows[1].state_name, "ACTIVE, HIGH");
    }

    #[test]
    fn csv_quotes_unknown_text_and_leaves_non_state_fields_empty() {
        let rows = records_for_channel(
            "Energy Model".into(),
            String::new(),
            "GPU, Energy".into(),
            "mJ".into(),
            99,
            Vec::new(),
        );
        let mut output = Vec::new();
        write_csv(&mut output, &rows).expect("write fixture");
        assert_eq!(
            String::from_utf8(output).expect("UTF-8 CSV"),
            "group,subgroup,channel,unit,kind,state_index,state_name\nEnergy Model,,\"GPU, Energy\",mJ,unknown:99,,\n"
        );
    }
}
