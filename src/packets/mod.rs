use thiserror::Error;

use crate::v5::J2000_EPOCH;

pub mod capture;
pub mod cdc;
pub mod cdc2;
pub mod controller;
pub mod dash;
pub mod device;
pub mod factory;
pub mod file;
pub mod kv;
pub mod log;
pub mod radio;
pub mod slot;
pub mod system;

#[repr(transparent)]
pub struct VarU16(u16);
impl VarU16 {
    /// Creates a new variable length u16.
    /// # Panics
    /// Panics if the value is too large to be encoded as a variable length u16.
    pub fn new(val: u16) -> Self {
        if val > (u16::MAX >> 1) {
            panic!("Value too large for variable length u16");
        }
        Self(val)
    }
}
impl Encode for VarU16 {
    fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        if self.0 > (u16::MAX >> 1) {
            return Err(EncodeError::VarShortTooLarge);
        }

        if self.0 > (u8::MAX >> 1) as _ {
            let mut val = self.0.to_le_bytes();
            val[0] |= 1 << 7;
            Ok(val.to_vec())
        } else {
            let val = self.0 as u8;
            Ok(vec![val])
        }
    }
}

pub(crate) fn j2000_timestamp() -> u32 {
    (chrono::Utc::now().timestamp() - J2000_EPOCH as i64) as u32
}

pub struct VarLengthString<const MAX_LEN: u32>(String);
impl<const MAX_LEN: u32> VarLengthString<MAX_LEN> {
    pub fn new(string: String) -> Result<Self, EncodeError> {
        if string.len() as u32 > MAX_LEN {
            return Err(EncodeError::StringTooLong);
        }

        Ok(Self(string))
    }
}
impl<const MAX_LEN: u32>  Encode for VarLengthString<MAX_LEN> {
    fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        let mut bytes = self.0.as_bytes().to_vec();
        bytes.push(0);
        Ok(bytes)
    }
}
/// A null-terminated fixed length string.
/// Once encoded, the size will be `LEN + 1` bytes.
pub struct TerminatedFixedLengthString<const LEN: usize>([u8; LEN]);
impl<const LEN: usize> TerminatedFixedLengthString<LEN> {
    pub fn new(string: String) -> Result<Self, EncodeError> {
        let mut encoded = [0u8; LEN];

        let string_bytes = string.into_bytes();
        if string_bytes.len() > encoded.len() {
            return Err(EncodeError::StringTooLong);
        }

        encoded[..string_bytes.len()].copy_from_slice(&string_bytes);

        Ok(Self(encoded))
    }
}
impl<const LEN: usize> Encode for TerminatedFixedLengthString<LEN> {
    fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        let mut encoded = Vec::from(self.0);
        encoded.push(0);
        Ok(encoded)
    }
}

pub struct UnterminatedFixedLengthString<const LEN: usize>([u8; LEN]);
impl<const LEN: usize> UnterminatedFixedLengthString<LEN> {
    pub fn new(string: String) -> Result<Self, EncodeError> {
        let mut encoded = [0u8; LEN];

        let string_bytes = string.into_bytes();
        if string_bytes.len() > encoded.len() {
            return Err(EncodeError::StringTooLong);
        }

        encoded[..string_bytes.len()].copy_from_slice(&string_bytes);

        Ok(Self(encoded))
    }
}
impl Encode for UnterminatedFixedLengthString<23> {
    fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        Ok(self.0.to_vec())
    }
}

#[derive(Error, Debug)]
pub enum EncodeError {
    #[error("String bytes are too long")]
    StringTooLong,
    #[error("Value too large for variable length u16")]
    VarShortTooLarge,
}

/// A trait that allows for encoding a structure into a byte sequence.
pub trait Encode {
    /// Encodes a structure into a byte sequence.
    fn encode(&self) -> Result<Vec<u8>, EncodeError>;
    fn into_encoded(self) -> Result<Vec<u8>, EncodeError>
    where
        Self: Sized,
    {
        self.encode()
    }
}
impl Encode for () {
    fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        Ok(Vec::new())
    }
}
impl Encode for Vec<u8> {
    fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        Ok(self.clone())
    }
}

/// Device-bound Communications Packet
///
/// This structure encodes a data payload and ID that is intended to be sent from
/// a host machine to a V5 device over the serial protocol. This is typically done
/// through either a [`CdcCommandPacket`] or a [`Cdc2CommandPacket`].
pub struct DeviceBoundPacket<P: Encode, const ID: u8> {
    /// Device-bound Packet Header
    ///
    /// This must be `Self::HEADER` or `[0xC9, 0x36, 0xB8, 0x47]`.
    header: [u8; 4],

    /// Packet Payload
    ///
    /// Contains data for a given packet that be encoded and sent over serial to the device.
    payload: P,
}
impl<P: Encode, const ID: u8> Encode for DeviceBoundPacket<P, ID> {
    fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(&self.header);
        encoded.push(ID);

        let size = VarU16::new(self.payload.encode()?.len() as u16);
        encoded.extend(size.encode()?);

        encoded.extend_from_slice(&self.payload.encode()?);
        Ok(encoded)
    }
}

impl<P: Encode, const ID: u8> DeviceBoundPacket<P, ID> {
    /// Header byte sequence used for all device-bound packets.
    pub const HEADER: [u8; 4] = [0xC9, 0x36, 0xB8, 0x47];

    /// Creates a new device-bound packet with a given generic payload type.
    pub fn new(payload: P) -> Self {
        Self {
            header: Self::HEADER,
            payload,
        }
    }
}

/// Host-bound Communications Packet
///
/// This structure encodes a data payload and ID that is intended to be sent from
/// a V5 device to a host machine over the serial protocol. This is typically done
/// through either a [`CdcReplyPacket`] or a [`Cdc2ReplyPacket`].
pub struct HostBoundPacket<P, const ID: u8> {
    /// Host-bound Packet Header
    ///
    /// This must be `Self::HEADER` or `[0xAA, 0x55]`.
    header: [u8; 2],

    /// Packet Payload
    ///
    /// Contains data for a given packet that be encoded and sent over serial to the host.
    payload: P,
}

impl<P, const ID: u8> HostBoundPacket<P, ID> {
    /// Header byte sequence used for all host-bound packets.
    pub const HEADER: [u8; 2] = [0xAA, 0x55];

    /// Creates a new host-bound packet with a given generic payload type.
    pub fn new(payload: P) -> Self {
        Self {
            header: Self::HEADER,
            payload,
        }
    }
}

pub struct Version {
    pub major: u8,
    pub minor: u8,
    pub build: u8,
    pub beta: u8,
}
impl Encode for Version {
    fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        Ok(vec![self.major, self.minor, self.build, self.beta])
    }
}
