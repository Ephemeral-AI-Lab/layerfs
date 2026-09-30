//! Closed DirectoryRoots key/value records and logical operation capacities.
use crate::error::{ContentError, ContentResult};
use crate::object::ObjectId;

use super::selection::StateScope;

/// Actual DirectoryRoots key width, including compact scope prefix.
pub const STATE_KEY_BYTES: usize = 25;
/// Profile key ceiling; the implemented table still requires exactly 25 bytes.
pub const STATE_MAX_KEY_BYTES: usize = 288;
/// Profile value ceiling; DirectoryRoots values are exactly 32 bytes.
pub const STATE_MAX_VALUE_BYTES: usize = 8192;
/// Maximum admitted records in one append or output page.
pub const STATE_MAX_PAGE_RECORDS: usize = 128;
/// Maximum encoded append or output page, including its header.
pub const STATE_MAX_PAGE_BYTES: usize = 65536;
/// Actual DirectoryRoots record width, including length framing.
pub const DIRECTORY_ROOT_RECORD_BYTES: usize = 63;
/// version1/scope81/count2/record-bytes4.
pub const STATE_APPEND_HEADER_BYTES: usize = 88;

/// token8/phase8/table1/positive serial8. Full selection is the supplied scope.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StateKey([u8; STATE_KEY_BYTES]);

impl StateKey {
    /// Builds one positive in-range serial under the selected compact prefix.
    pub fn directory_root(scope: &StateScope, serial: u64) -> ContentResult<Self> {
        if serial == 0 || serial > i64::MAX as u64 {
            return Err(ContentError::InvalidOrderingRecord("state serial"));
        }
        let mut bytes = [0; STATE_KEY_BYTES];
        bytes[..8].copy_from_slice(&scope.selection().token().to_be_bytes());
        bytes[8..16].copy_from_slice(&scope.phase().to_be_bytes());
        bytes[16] = scope.table().code();
        bytes[17..].copy_from_slice(&serial.to_be_bytes());
        Ok(Self(bytes))
    }

    /// Checks exact width, serial and prefix against the supplied full scope.
    pub fn decode(scope: &StateScope, bytes: &[u8]) -> ContentResult<Self> {
        if bytes.len() > STATE_MAX_KEY_BYTES {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "indexed_state.key_bytes",
                limit: STATE_MAX_KEY_BYTES as u64,
                actual: bytes.len() as u64,
            });
        }
        let bytes: [u8; STATE_KEY_BYTES] = bytes
            .try_into()
            .map_err(|_| ContentError::InvalidOrderingRecord("state key width"))?;
        let serial = u64::from_be_bytes(bytes[17..].try_into().unwrap());
        let expected = Self::directory_root(scope, serial)?;
        if bytes != expected.0 {
            return Err(ContentError::InvalidOrderingRecord("state key selection"));
        }
        Ok(expected)
    }

    /// Encoded token8/phase8/table1/serial8 key.
    pub const fn as_bytes(&self) -> &[u8; STATE_KEY_BYTES] {
        &self.0
    }

    /// The complete checked local inode serial.
    pub fn serial(self) -> u64 {
        u64::from_be_bytes(self.0[17..].try_into().unwrap())
    }
}

/// Fixed native key/value ownership; the framed record is exactly 63 bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StateRecord {
    key: StateKey,
    root: ObjectId,
}

impl StateRecord {
    /// Associates an already checked typed key with its immutable canonical root.
    pub const fn new(key: StateKey, root: ObjectId) -> Self {
        Self { key, root }
    }

    /// Checks a serial and associates its root under the exact supplied scope.
    pub fn directory_root(scope: &StateScope, serial: u64, root: ObjectId) -> ContentResult<Self> {
        Ok(Self::new(StateKey::directory_root(scope, serial)?, root))
    }

    /// The complete typed key.
    pub const fn key(self) -> StateKey {
        self.key
    }

    /// The exact immutable directory root identity.
    pub const fn root(self) -> ObjectId {
        self.root
    }

    /// Encodes key-length2/key25/value-length4/root32 without allocation.
    pub fn encode(self) -> [u8; DIRECTORY_ROOT_RECORD_BYTES] {
        let mut bytes = [0; DIRECTORY_ROOT_RECORD_BYTES];
        bytes[..2].copy_from_slice(&(STATE_KEY_BYTES as u16).to_be_bytes());
        bytes[2..27].copy_from_slice(self.key.as_bytes());
        bytes[27..31].copy_from_slice(&32u32.to_be_bytes());
        bytes[31..].copy_from_slice(self.root.as_bytes());
        bytes
    }

    /// Checks complete length framing and scope before exposing a typed record.
    pub fn decode(scope: &StateScope, bytes: &[u8]) -> ContentResult<Self> {
        if bytes.len() != DIRECTORY_ROOT_RECORD_BYTES {
            return Err(ContentError::InvalidOrderingRecord("state record width"));
        }
        if bytes[..2] != (STATE_KEY_BYTES as u16).to_be_bytes()
            || bytes[27..31] != 32u32.to_be_bytes()
        {
            return Err(ContentError::InvalidOrderingRecord("state record framing"));
        }
        Ok(Self::new(
            StateKey::decode(scope, &bytes[2..27])?,
            ObjectId::from_bytes(&bytes[31..])?,
        ))
    }
}

/// Logical capacity declared before canonical effects; physical ownership is separate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StateCapacity {
    records: u64,
    encoded_bytes: u64,
}

impl StateCapacity {
    /// Reports independently admitted count and encoded-record-byte allowances.
    /// This metadata does not acquire or prove a physical resource reservation.
    pub const fn new(records: u64, encoded_bytes: u64) -> ContentResult<Self> {
        Ok(Self {
            records,
            encoded_bytes,
        })
    }

    /// Admitted population count, distinct from a single page's limit.
    pub const fn records(self) -> u64 {
        self.records
    }

    /// Admitted encoded record population, excluding per-page headers.
    pub const fn encoded_bytes(self) -> u64 {
        self.encoded_bytes
    }

    /// Refuses a declared DirectoryRoots shape before any canonical effect.
    pub fn check_requested(self, records: usize) -> ContentResult<()> {
        let records = u64::try_from(records).map_err(|_| ContentError::LengthOverflow)?;
        let bytes = records
            .checked_mul(DIRECTORY_ROOT_RECORD_BYTES as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if records > self.records {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "indexed_state.records",
                limit: self.records,
                actual: records,
            });
        }
        if bytes > self.encoded_bytes {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "indexed_state.record_bytes",
                limit: self.encoded_bytes,
                actual: bytes,
            });
        }
        Ok(())
    }
}
