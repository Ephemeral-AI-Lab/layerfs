//! Compact ordering-record grammar for reference effects and final values.
//!
//! One row is fixed 96 bytes: serial, version, tag, declared length, a typed-value
//! flag, one authoritative tally and an optional 73-byte inode value. No field
//! overlaps another, so a decoded row always describes exactly the bytes it was
//! given. Rows carry no Workspace node, checkpoint receipt or temporary
//! inode-record identity; those had no remaining semantic consumer once final
//! typed values and reference effects became the operation's inputs.
//!
//! Layout, all fields big endian: bytes 0..8 hold the serial; byte 8 the version;
//! byte 9 the tag (1 for a new-inode binding count, 2 for an existing inode's
//! effect total); bytes 10..12 the declared row length (96); byte 12 the
//! typed-value flag (0 = no value, 1 = a 73-byte value follows); bytes 13..21 the
//! tally (retained bindings, or a signed effect total); bytes 21..94 the inode
//! value, present only when the flag is set; bytes 94..96 reserved and zero.
//!
//! The tally is the only count this grammar trusts. For a `Count` row the typed
//! value is stored exactly as supplied, including its own count field, which the
//! operation never uses: a new inode's count is the tally, and an existing
//! inode's count comes from its stored record plus the effect total.

use crate::error::{ContentError, ContentResult};
use crate::object::inode_leaf::{decode_inode_value, encode_inode_value, InodeValue};

/// Width of one ordering row.
pub const ROW_BYTES: usize = 96;
/// Grammar version of one ordering row.
pub const ROW_VERSION: u8 = 1;
/// Tag of a new inode's retained-binding row.
pub const TAG_COUNT: u8 = 1;
/// Tag of an existing inode's effect row.
pub const TAG_EFFECT: u8 = 2;

const FLAG_OFFSET: usize = 12;
const TALLY_OFFSET: usize = 13;
const VALUE_OFFSET: usize = 21;
const VALUE_END: usize = VALUE_OFFSET + crate::object::inode_leaf::INODE_VALUE_BYTES;
const RESERVED_OFFSET: usize = VALUE_END;

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
                bytes[TALLY_OFFSET..TALLY_OFFSET + 8].copy_from_slice(&count.to_be_bytes());
                write_value(&mut bytes, value);
            }
            Self::Effect {
                serial,
                value,
                delta,
            } => {
                bytes[..8].copy_from_slice(&serial.to_be_bytes());
                bytes[9] = TAG_EFFECT;
                bytes[TALLY_OFFSET..TALLY_OFFSET + 8].copy_from_slice(&delta.to_be_bytes());
                write_value(&mut bytes, value);
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
        if bytes[RESERVED_OFFSET..].iter().any(|byte| *byte != 0) {
            return Err(ContentError::InvalidOrderingRecord("reserved"));
        }
        let value = match bytes[FLAG_OFFSET] {
            0 => {
                if bytes[VALUE_OFFSET..VALUE_END].iter().any(|byte| *byte != 0) {
                    return Err(ContentError::InvalidOrderingRecord("absent value"));
                }
                None
            }
            1 => {
                let mut raw = [0_u8; crate::object::inode_leaf::INODE_VALUE_BYTES];
                raw.copy_from_slice(&bytes[VALUE_OFFSET..VALUE_END]);
                Some(decode_inode_value(&raw)?)
            }
            _ => return Err(ContentError::InvalidOrderingRecord("value flag")),
        };
        let tally = bytes[TALLY_OFFSET..TALLY_OFFSET + 8]
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?;
        match bytes[9] {
            TAG_COUNT => Ok(Self::Count {
                serial,
                value,
                count: u64::from_be_bytes(tally),
            }),
            TAG_EFFECT => Ok(Self::Effect {
                serial,
                value,
                delta: i64::from_be_bytes(tally),
            }),
            _ => Err(ContentError::InvalidOrderingRecord("tag")),
        }
    }
}

fn write_value(bytes: &mut [u8; ROW_BYTES], value: Option<InodeValue>) {
    let Some(value) = value else {
        return;
    };
    bytes[VALUE_OFFSET..VALUE_END].copy_from_slice(&encode_inode_value(value));
    bytes[FLAG_OFFSET] = 1;
}
