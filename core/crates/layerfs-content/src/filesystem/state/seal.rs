//! Incremental acknowledged-record digest and immutable exact phase seal.
use crate::error::{ContentError, ContentResult};

use super::{
    records::{
        DIRECTORY_ROOT_RECORD_BYTES, STATE_APPEND_HEADER_BYTES, STATE_MAX_PAGE_BYTES,
        STATE_MAX_PAGE_RECORDS,
    },
    StateKey, StateRecord, StateScope,
};

/// Versioned exact scope, terminal counts/bytes and digest width.
pub const STATE_SEAL_BYTES: usize = 130;
const DOMAIN: &[u8] = b"layerfs/indexed-state/v1\0";

/// Immutable acknowledged scope, totals and complete ordered-record digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateSeal {
    scope: StateScope,
    records: u64,
    encoded_bytes: u64,
    digest: [u8; 32],
}

impl StateSeal {
    /// Exact issued identity, owner binding, phase and table.
    pub fn scope(&self) -> &StateScope {
        &self.scope
    }
    /// Total acknowledged records in the sealed table.
    pub const fn records(&self) -> u64 {
        self.records
    }
    /// Total encoded record bytes, excluding per-page headers.
    pub const fn encoded_bytes(&self) -> u64 {
        self.encoded_bytes
    }
    /// Complete private BLAKE3 transcript digest.
    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    /// version1/scope81/total-count8/total-record-bytes8/digest32.
    pub fn encode(&self) -> [u8; STATE_SEAL_BYTES] {
        let mut bytes = [0; STATE_SEAL_BYTES];
        bytes[0] = 1;
        bytes[1..82].copy_from_slice(&self.scope.as_bytes());
        bytes[82..90].copy_from_slice(&self.records.to_be_bytes());
        bytes[90..98].copy_from_slice(&self.encoded_bytes.to_be_bytes());
        bytes[98..].copy_from_slice(&self.digest);
        bytes
    }

    /// Checks framing, scope and terminal arithmetic against an issued owner.
    /// Callers still compare this seal with the owner's acknowledged exact seal.
    pub fn decode(scope: StateScope, bytes: &[u8]) -> ContentResult<Self> {
        if bytes.len() != STATE_SEAL_BYTES || bytes[0] != 1 || bytes[1..82] != scope.as_bytes() {
            return Err(ContentError::InvalidOrderingRecord("state seal framing"));
        }
        let records = u64::from_be_bytes(bytes[82..90].try_into().unwrap());
        let encoded_bytes = u64::from_be_bytes(bytes[90..98].try_into().unwrap());
        if records.checked_mul(DIRECTORY_ROOT_RECORD_BYTES as u64) != Some(encoded_bytes) {
            return Err(ContentError::InvalidOrderingRecord("state seal totals"));
        }
        Ok(Self {
            scope,
            records,
            encoded_bytes,
            digest: bytes[98..].try_into().unwrap(),
        })
    }
}

/// Digest/totals advance only after one acknowledged ordered append.
pub struct StateLedger {
    scope: StateScope,
    digest: blake3::Hasher,
    records: u64,
    encoded_bytes: u64,
    last: Option<StateKey>,
}

impl StateLedger {
    /// Starts an empty transcript bound to the exact immutable scope.
    pub fn new(scope: StateScope) -> Self {
        let mut digest = blake3::Hasher::new();
        digest.update(DOMAIN);
        digest.update(&scope.as_bytes());
        Self {
            scope,
            digest,
            records: 0,
            encoded_bytes: 0,
            last: None,
        }
    }

    /// The selected operation/owner/phase/table context.
    pub fn scope(&self) -> &StateScope {
        &self.scope
    }
    /// Records already acknowledged by the owning provider.
    pub const fn records(&self) -> u64 {
        self.records
    }
    /// Encoded record bytes already acknowledged, excluding page headers.
    pub const fn encoded_bytes(&self) -> u64 {
        self.encoded_bytes
    }
    /// Last acknowledged complete key, or none for an empty table.
    pub const fn last(&self) -> Option<StateKey> {
        self.last
    }

    /// All widths/order/count/bytes and terminal arithmetic precede append effects.
    pub fn validate_append(&self, records: &[StateRecord]) -> ContentResult<()> {
        if records.len() > STATE_MAX_PAGE_RECORDS {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "indexed_state.append_records",
                limit: STATE_MAX_PAGE_RECORDS as u64,
                actual: records.len() as u64,
            });
        }
        let bytes = records
            .len()
            .checked_mul(DIRECTORY_ROOT_RECORD_BYTES)
            .and_then(|bytes| bytes.checked_add(STATE_APPEND_HEADER_BYTES))
            .ok_or(ContentError::LengthOverflow)?;
        if bytes > STATE_MAX_PAGE_BYTES {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "indexed_state.append_bytes",
                limit: STATE_MAX_PAGE_BYTES as u64,
                actual: bytes as u64,
            });
        }
        let mut last = self.last;
        for record in records {
            StateKey::decode(&self.scope, record.key().as_bytes())?;
            if last.is_some_and(|key| key >= record.key()) {
                return Err(ContentError::InvalidOrderingRecord("state append order"));
            }
            last = Some(record.key());
        }
        self.records
            .checked_add(records.len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        self.encoded_bytes
            .checked_add((bytes - STATE_APPEND_HEADER_BYTES) as u64)
            .ok_or(ContentError::LengthOverflow)?;
        Ok(())
    }

    /// Advances digest/totals once the whole checked ordered batch is acknowledged.
    pub fn acknowledge(&mut self, records: &[StateRecord]) -> ContentResult<()> {
        self.validate_append(records)?;
        for record in records {
            self.digest.update(&record.encode());
            self.last = Some(record.key());
        }
        self.records += records.len() as u64;
        self.encoded_bytes += (records.len() * DIRECTORY_ROOT_RECORD_BYTES) as u64;
        Ok(())
    }

    /// Encodes a checked version1/scope81/count2/record-bytes4 batch header.
    pub fn append_header(
        &self,
        records: &[StateRecord],
    ) -> ContentResult<[u8; STATE_APPEND_HEADER_BYTES]> {
        self.validate_append(records)?;
        let mut bytes = [0; STATE_APPEND_HEADER_BYTES];
        bytes[0] = 1;
        bytes[1..82].copy_from_slice(&self.scope.as_bytes());
        bytes[82..84].copy_from_slice(&(records.len() as u16).to_be_bytes());
        bytes[84..]
            .copy_from_slice(&((records.len() * DIRECTORY_ROOT_RECORD_BYTES) as u32).to_be_bytes());
        Ok(bytes)
    }

    /// Snapshots the exact digest with terminal record and byte totals.
    pub fn seal(&self) -> StateSeal {
        let mut digest = self.digest.clone();
        digest.update(&self.records.to_be_bytes());
        digest.update(&self.encoded_bytes.to_be_bytes());
        StateSeal {
            scope: self.scope.clone(),
            records: self.records,
            encoded_bytes: self.encoded_bytes,
            digest: *digest.finalize().as_bytes(),
        }
    }
}
