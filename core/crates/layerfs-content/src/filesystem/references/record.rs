//! Compact ordering-record grammar for reference effects and final values.
//!
//! One row is fixed 88 bytes: serial, version, tag, declared length, an optional
//! typed value and the pending tally that tag selects. Rows carry no Workspace
//! node, checkpoint receipt or temporary inode-record identity; those had no
//! remaining semantic consumer once final typed values and reference effects
//! became the operation's inputs.
//!
//! Layout, all fields big endian: bytes 0..8 hold the serial; byte 8 the
//! version; byte 9 the tag (1 for a new-inode binding count, 2 for an existing
//! inode's effect total); bytes 10..12 the declared row length; byte 12 the inode
//! kind code, zero when the row carries no typed value; byte 13 the typed-value
//! flag; bytes 14..22 the tally (retained bindings or a signed effect total);
//! bytes 22..54 the content root; bytes 54..86 the attribute root; bytes 86..88
//! reserved and zero.

use crate::error::{ContentError, ContentResult};
use crate::object::inode_leaf::{decode_inode_value, encode_inode_value, InodeValue};

/// Width of one ordering row.
pub const ROW_BYTES: usize = 88;
/// Grammar version of one ordering row.
pub const ROW_VERSION: u8 = 1;
/// Tag of a new inode's retained-binding row.
pub const TAG_COUNT: u8 = 1;
/// Tag of an existing inode's effect row.
pub const TAG_EFFECT: u8 = 2;

/// One pending inode state, as the operation holds it and as it spills it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Row {
    /// A newly allocated inode: its typed value and retained binding count.
    Count {
        /// Inode serial.
        serial: u64,
        /// Typed final value, once the caller supplied one.
        value: Option<InodeValue>,
        /// Bindings retained for this inode.
        count: u64,
    },
    /// An existing inode: its signed effect total and any supplied typed value.
    Effect {
        /// Inode serial.
        serial: u64,
        /// Typed final value, when the caller supplied one.
        value: Option<InodeValue>,
        /// Signed effect total observed so far.
        delta: i64,
    },
}

impl Row {
    /// The serial this row addresses.
    pub const fn serial(self) -> u64 {
        match self {
            Self::Count { serial, .. } | Self::Effect { serial, .. } => serial,
        }
    }

    /// The typed value carried by this row, when any.
    pub const fn value(self) -> Option<InodeValue> {
        match self {
            Self::Count { value, .. } | Self::Effect { value, .. } => value,
        }
    }

    /// Encodes the row into its fixed-width form.
    pub fn encode(self) -> ContentResult<[u8; ROW_BYTES]> {
        let mut bytes = [0_u8; ROW_BYTES];
        bytes[8] = ROW_VERSION;
        bytes[10..12].copy_from_slice(&(ROW_BYTES as u16).to_be_bytes());
        match self {
            Self::Count {
                serial,
                value,
                count,
            } => {
                bytes[..8].copy_from_slice(&serial.to_be_bytes());
                bytes[9] = TAG_COUNT;
                bytes[14..22].copy_from_slice(&count.to_be_bytes());
                write_value(&mut bytes, value)?;
            }
            Self::Effect {
                serial,
                value,
                delta,
            } => {
                bytes[..8].copy_from_slice(&serial.to_be_bytes());
                bytes[9] = TAG_EFFECT;
                bytes[14..22].copy_from_slice(&delta.to_be_bytes());
                write_value(&mut bytes, value)?;
            }
        }
        Ok(bytes)
    }

    /// Decodes one fixed-width row, rejecting every malformed field.
    pub fn decode(bytes: &[u8; ROW_BYTES]) -> ContentResult<Self> {
        let serial = u64::from_be_bytes(
            bytes[..8]
                .try_into()
                .map_err(|_| ContentError::UnexpectedEof)?,
        );
        if serial == 0 {
            return Err(ContentError::InvalidOrderingRecord("serial"));
        }
        if bytes[8] != ROW_VERSION {
            return Err(ContentError::InvalidOrderingRecord("version"));
        }
        if usize::from(u16::from_be_bytes([bytes[10], bytes[11]])) != ROW_BYTES {
            return Err(ContentError::InvalidOrderingRecord("length"));
        }
        if bytes[86] != 0 || bytes[87] != 0 {
            return Err(ContentError::InvalidOrderingRecord("reserved"));
        }
        let value = match bytes[13] {
            0 => {
                if bytes[12] != 0 || bytes[22..86].iter().any(|byte| *byte != 0) {
                    return Err(ContentError::InvalidOrderingRecord("absent value"));
                }
                None
            }
            1 => Some(decode_inode_value(&bytes[12..85])?),
            _ => return Err(ContentError::InvalidOrderingRecord("value flag")),
        };
        match bytes[9] {
            TAG_COUNT => Ok(Self::Count {
                serial,
                value,
                count: u64::from_be_bytes(
                    bytes[14..22]
                        .try_into()
                        .map_err(|_| ContentError::UnexpectedEof)?,
                ),
            }),
            TAG_EFFECT => Ok(Self::Effect {
                serial,
                value,
                delta: i64::from_be_bytes(
                    bytes[14..22]
                        .try_into()
                        .map_err(|_| ContentError::UnexpectedEof)?,
                ),
            }),
            _ => Err(ContentError::InvalidOrderingRecord("tag")),
        }
    }
}

fn write_value(bytes: &mut [u8; ROW_BYTES], value: Option<InodeValue>) -> ContentResult<()> {
    let Some(value) = value else {
        return Ok(());
    };
    bytes[12..85].copy_from_slice(&encode_inode_value(value));
    bytes[13] = 1;
    Ok(())
}
