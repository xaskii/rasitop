use serde::Serialize;

use super::error::{Result, SmcError};
use super::protocol::{SmcConnection, SmcKeyInfo};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) struct SmcKey(u32);

impl SmcKey {
    pub const fn from_bytes(bytes: [u8; 4]) -> Self {
        Self(u32::from_be_bytes(bytes))
    }

    pub fn name(self) -> String {
        String::from_utf8_lossy(&self.0.to_be_bytes()).into_owned()
    }

    pub(super) const fn raw(self) -> u32 {
        self.0
    }

    pub(super) const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Decoder {
    Float,
    SignedFixed7_8,
    UnsignedFixed14_2,
    Unsigned8,
    Unsigned16,
    Unsigned32,
}

impl Decoder {
    pub fn from_info(info: SmcKeyInfo) -> Result<Self> {
        let (decoder, expected_size) = match info.data_type.to_be_bytes() {
            [b'f', b'l', b't', b' '] => (Self::Float, 4),
            [b's', b'p', b'7', b'8'] => (Self::SignedFixed7_8, 2),
            [b'f', b'p', b'e', b'2'] => (Self::UnsignedFixed14_2, 2),
            [b'u', b'i', b'8', b' '] => (Self::Unsigned8, 1),
            [b'u', b'i', b'1', b'6'] => (Self::Unsigned16, 2),
            [b'u', b'i', b'3', b'2'] => (Self::Unsigned32, 4),
            data_type => {
                return Err(SmcError::UnsupportedDataType {
                    data_type: String::from_utf8_lossy(&data_type).into_owned(),
                });
            }
        };
        if info.data_size as usize != expected_size {
            return Err(SmcError::DataTypeSize {
                data_type: data_type_name(info),
                expected: expected_size,
                actual: info.data_size,
            });
        }
        Ok(decoder)
    }

    pub fn decode(self, bytes: &[u8]) -> Result<f64> {
        let value = match self {
            Self::Float => f32::from_le_bytes(exact_bytes(bytes)?) as f64,
            Self::SignedFixed7_8 => i16::from_be_bytes(exact_bytes(bytes)?) as f64 / 256.0,
            Self::UnsignedFixed14_2 => u16::from_be_bytes(exact_bytes(bytes)?) as f64 / 4.0,
            Self::Unsigned8 => f64::from(exact_bytes::<1>(bytes)?[0]),
            Self::Unsigned16 => f64::from(u16::from_be_bytes(exact_bytes(bytes)?)),
            Self::Unsigned32 => u32::from_be_bytes(exact_bytes(bytes)?) as f64,
        };
        if !value.is_finite() {
            return Err(SmcError::NonFiniteValue);
        }
        Ok(value)
    }
}

fn exact_bytes<const N: usize>(bytes: &[u8]) -> Result<[u8; N]> {
    bytes.try_into().map_err(|_| SmcError::ValueSize {
        expected: N,
        actual: bytes.len(),
    })
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ResolvedKey {
    key: SmcKey,
    info: SmcKeyInfo,
    decoder: Decoder,
}

impl ResolvedKey {
    pub fn resolve(connection: &SmcConnection, key: SmcKey) -> Result<Self> {
        let info = connection.read_key_info(key)?;
        let decoder = Decoder::from_info(info)?;
        Ok(Self { key, info, decoder })
    }

    pub fn read(self, connection: &SmcConnection) -> Result<f64> {
        let bytes = connection.read_bytes(self.key, self.info)?;
        self.decoder.decode(&bytes[..self.info.data_size as usize])
    }
}

pub(super) fn data_type_name(info: SmcKeyInfo) -> String {
    String::from_utf8_lossy(&info.data_type.to_be_bytes()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{Decoder, SmcError, SmcKeyInfo};

    fn info(data_type: [u8; 4], data_size: u32) -> SmcKeyInfo {
        SmcKeyInfo {
            data_size,
            data_type: u32::from_be_bytes(data_type),
            data_attributes: 0,
        }
    }

    #[test]
    fn decodes_every_supported_stats_value_type() {
        let fixtures: &[([u8; 4], &[u8], f64)] = &[
            (*b"flt ", &42.5_f32.to_le_bytes(), 42.5),
            (*b"sp78", &[0x2a, 0x80], 42.5),
            (*b"sp78", &[0xff, 0x80], -0.5),
            (*b"fpe2", &[0x00, 0xaa], 42.5),
            (*b"ui8 ", &[42], 42.0),
            (*b"ui16", &[0x01, 0x02], 258.0),
            (*b"ui32", &[0x01, 0x02, 0x03, 0x04], 16_909_060.0),
        ];

        for &(data_type, bytes, expected) in fixtures {
            let decoder = Decoder::from_info(info(data_type, bytes.len() as u32))
                .expect("supported decoder fixture");
            assert_eq!(decoder.decode(bytes).expect("decode fixture"), expected);
        }
    }

    #[test]
    fn rejects_wrong_sizes_unknown_types_and_non_finite_floats() {
        assert!(Decoder::from_info(info(*b"flt ", 2)).is_err());
        assert!(matches!(
            Decoder::from_info(info(*b"flag", 1)),
            Err(SmcError::UnsupportedDataType { data_type }) if data_type == "flag"
        ));
        let decoder = Decoder::from_info(info(*b"flt ", 4)).expect("float decoder");
        assert!(matches!(
            decoder.decode(&f32::NAN.to_le_bytes()),
            Err(SmcError::NonFiniteValue)
        ));
        assert!(matches!(
            decoder.decode(&f32::INFINITY.to_le_bytes()),
            Err(SmcError::NonFiniteValue)
        ));
        assert!(matches!(
            decoder.decode(&[0; 3]),
            Err(SmcError::ValueSize {
                expected: 4,
                actual: 3
            })
        ));
    }
}
