//! Closed exclusive non-file binding claims and logical class admission.
use super::{StateScope, StateTable, STATE_KEY_BYTES};
use crate::error::{ContentError, ContentResult};

/// key-length2/key25/value-length4/exclusive-class1.
pub const CLAIM_RECORD_BYTES: usize = 32;
/// version1/scope81/count2/record-bytes4.
pub const CLAIM_APPEND_HEADER_BYTES: usize = 88;

/// token8/phase8/table2/positive serial8; no root-value codec is accepted.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ClaimKey([u8; STATE_KEY_BYTES]);
impl ClaimKey {
    /// Selects one serial under an issued BindingClaims scope.
    pub fn new(scope: &StateScope, serial: u64) -> ContentResult<Self> {
        if scope.table() != StateTable::BindingClaims {
            return Err(ContentError::InvalidOrderingRecord("claim table"));
        }
        if serial == 0 || serial > i64::MAX as u64 {
            return Err(ContentError::InvalidOrderingRecord("claim serial"));
        }
        let mut bytes = [0; STATE_KEY_BYTES];
        bytes[..8].copy_from_slice(&scope.selection().token().to_be_bytes());
        bytes[8..16].copy_from_slice(&scope.phase().to_be_bytes());
        bytes[16] = StateTable::BindingClaims.code();
        bytes[17..].copy_from_slice(&serial.to_be_bytes());
        Ok(Self(bytes))
    }
    /// Checks exact width, selected prefix and positive serial before use.
    pub fn decode(scope: &StateScope, bytes: &[u8]) -> ContentResult<Self> {
        let bytes: [u8; STATE_KEY_BYTES] = bytes
            .try_into()
            .map_err(|_| ContentError::InvalidOrderingRecord("claim key width"))?;
        let expected = Self::new(scope, u64::from_be_bytes(bytes[17..].try_into().unwrap()))?;
        if bytes != expected.0 {
            return Err(ContentError::InvalidOrderingRecord("claim key selection"));
        }
        Ok(expected)
    }
    /// Complete fixed selected key.
    pub const fn as_bytes(&self) -> &[u8; STATE_KEY_BYTES] {
        &self.0
    }
    /// Positive local serial.
    pub fn serial(self) -> u64 {
        u64::from_be_bytes(self.0[17..].try_into().unwrap())
    }
}

/// A typed exclusive claim. The only value is class1, implied by this type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaimRecord {
    key: ClaimKey,
}
impl ClaimRecord {
    /// Uses an already checked exclusive key, with no arbitrary value field.
    pub const fn new(key: ClaimKey) -> Self {
        Self { key }
    }
    /// Checks the exact scope and serial.
    pub fn exclusive(scope: &StateScope, serial: u64) -> ContentResult<Self> {
        Ok(Self::new(ClaimKey::new(scope, serial)?))
    }
    /// Complete selected key.
    pub const fn key(self) -> ClaimKey {
        self.key
    }
    /// Fixed frame32, with exclusive class byte1.
    pub fn encode(self) -> [u8; CLAIM_RECORD_BYTES] {
        let mut bytes = [0; CLAIM_RECORD_BYTES];
        bytes[..2].copy_from_slice(&(STATE_KEY_BYTES as u16).to_be_bytes());
        bytes[2..27].copy_from_slice(self.key.as_bytes());
        bytes[27..31].copy_from_slice(&1u32.to_be_bytes());
        bytes[31] = 1;
        bytes
    }
    /// Refuses wrong framing, class, selected scope or serial before use.
    pub fn decode(scope: &StateScope, bytes: &[u8]) -> ContentResult<Self> {
        if bytes.len() != CLAIM_RECORD_BYTES
            || bytes[..2] != 25u16.to_be_bytes()
            || bytes[27..31] != 1u32.to_be_bytes()
            || bytes[31] != 1
        {
            return Err(ContentError::InvalidOrderingRecord("claim record framing"));
        }
        Ok(Self::new(ClaimKey::decode(scope, &bytes[2..27])?))
    }
}

/// Logical rows and frame32 bytes; native/physical ownership is separate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaimCapacity {
    records: u64,
    encoded_bytes: u64,
}
impl ClaimCapacity {
    /// Reports previously admitted count and byte classes independently.
    pub const fn new(records: u64, encoded_bytes: u64) -> ContentResult<Self> {
        Ok(Self {
            records,
            encoded_bytes,
        })
    }
    /// Admitted row population.
    pub const fn records(self) -> u64 {
        self.records
    }
    /// Admitted framed record bytes, excluding headers.
    pub const fn encoded_bytes(self) -> u64 {
        self.encoded_bytes
    }
    /// Checks the declared maximum before construction or claim effects.
    pub fn check_requested(self, records: usize) -> ContentResult<()> {
        let records = u64::try_from(records).map_err(|_| ContentError::LengthOverflow)?;
        let bytes = records
            .checked_mul(CLAIM_RECORD_BYTES as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if records > self.records {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_claims.records",
                limit: self.records,
                actual: records,
            });
        }
        if bytes > self.encoded_bytes {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_claims.record_bytes",
                limit: self.encoded_bytes,
                actual: bytes,
            });
        }
        Ok(())
    }
}
