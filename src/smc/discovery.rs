use std::fmt::Write as _;

use serde::Serialize;

use super::error::{Result, SmcError};
use super::protocol::{SmcConnection, cpu_brand};
use super::value::{Decoder, data_type_name};

#[derive(Debug, Serialize)]
pub struct DiagnosticReport {
    pub chip: String,
    pub key_count: u32,
    pub keys: Vec<DiagnosticKey>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticKey {
    pub index: u32,
    pub key: String,
    pub data_type: String,
    pub data_size: u32,
    pub raw_hex: String,
    pub value: Option<f64>,
    pub error: Option<String>,
}

/// Enumerates the complete SMC catalog for an explicit diagnostics request.
/// This is intentionally separate from normal provider initialization.
pub fn discover() -> Result<DiagnosticReport> {
    let chip = cpu_brand()?;
    let connection = SmcConnection::open()?;
    let key_count = connection.key_count()?;
    let mut keys = Vec::with_capacity(key_count as usize);

    for index in 0..key_count {
        keys.push(read_diagnostic_key(&connection, index));
    }

    Ok(DiagnosticReport {
        chip,
        key_count,
        keys,
    })
}

fn read_diagnostic_key(connection: &SmcConnection, index: u32) -> DiagnosticKey {
    let key = match connection.key_by_index(index) {
        Ok(key) => key,
        Err(error) => return failed(index, String::new(), error),
    };
    let info = match connection.read_key_info(key) {
        Ok(info) => info,
        Err(error) => return failed(index, key.name(), error),
    };
    let bytes = match connection.read_bytes(key, info) {
        Ok(bytes) => bytes,
        Err(error) => {
            return DiagnosticKey {
                index,
                key: key.name(),
                data_type: data_type_name(info),
                data_size: info.data_size,
                raw_hex: String::new(),
                value: None,
                error: Some(error.to_string()),
            };
        }
    };
    let raw = &bytes[..info.data_size as usize];
    let decoded = Decoder::from_info(info).and_then(|decoder| decoder.decode(raw));
    DiagnosticKey {
        index,
        key: key.name(),
        data_type: data_type_name(info),
        data_size: info.data_size,
        raw_hex: hex(raw),
        value: decoded.as_ref().ok().copied(),
        error: decoded.err().map(|error| error.to_string()),
    }
}

fn failed(index: u32, key: String, error: SmcError) -> DiagnosticKey {
    DiagnosticKey {
        index,
        key,
        data_type: String::new(),
        data_size: 0,
        raw_hex: String::new(),
        value: None,
        error: Some(error.to_string()),
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::hex;

    #[test]
    fn raw_bytes_are_stable_lowercase_hex() {
        assert_eq!(hex(&[0, 0xab, 0xff]), "00abff");
    }
}
