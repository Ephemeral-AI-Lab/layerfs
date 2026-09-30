//! Incremental acknowledged-record digest and immutable exact phase seal.
use crate::error::{ContentError, ContentResult};

use super::{
    claim_records::{CLAIM_APPEND_HEADER_BYTES, CLAIM_RECORD_BYTES},
    ClaimKey, ClaimRecord, StateScope, STATE_MAX_PAGE_BYTES, STATE_MAX_PAGE_RECORDS,
};

/// Versioned exact scope, terminal counts/bytes and digest width.
pub const CLAIM_SEAL_BYTES: usize = 130;
const DOMAIN: &[u8] = b"layerfs/binding-claims/v1\0";

/// Immutable acknowledged scope, totals and complete ordered-record digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimSeal {
    scope: StateScope,
    records: u64,
    encoded_bytes: u64,
    digest: [u8; 32],
}

impl ClaimSeal {
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
    pub fn encode(&self) -> [u8; CLAIM_SEAL_BYTES] {
        let mut bytes = [0; CLAIM_SEAL_BYTES];
        bytes[0] = 1;
        bytes[1..82].copy_from_slice(&self.scope.as_bytes());
        bytes[82..90].copy_from_slice(&self.records.to_be_bytes());
        bytes[90..98].copy_from_slice(&self.encoded_bytes.to_be_bytes());
        bytes[98..].copy_from_slice(&self.digest);
        bytes
    }

    /// Checks framing, scope and terminal arithmetic against an issued owner.
    /// Callers still compare this seal with the owner's acknowledged exact seal.
    pub fn decode(scope: &StateScope, bytes: &[u8]) -> ContentResult<Self> {
        if scope.table() != super::StateTable::BindingClaims {
            return Err(ContentError::InvalidOrderingRecord("claim table"));
        }
        if bytes.len() != CLAIM_SEAL_BYTES || bytes[0] != 1 || bytes[1..82] != scope.as_bytes() {
            return Err(ContentError::InvalidOrderingRecord("claim seal framing"));
        }
        let records = u64::from_be_bytes(bytes[82..90].try_into().unwrap());
        let encoded_bytes = u64::from_be_bytes(bytes[90..98].try_into().unwrap());
        if records.checked_mul(CLAIM_RECORD_BYTES as u64) != Some(encoded_bytes) {
            return Err(ContentError::InvalidOrderingRecord("claim seal totals"));
        }
        Ok(Self {
            scope: scope.clone(),
            records,
            encoded_bytes,
            digest: bytes[98..].try_into().unwrap(),
        })
    }
}

/// Digest/totals advance only after one acknowledged ordered append.
pub struct ClaimLedger {
    scope: StateScope,
    digest: blake3::Hasher,
    records: u64,
    encoded_bytes: u64,
    last: Option<ClaimKey>,
}

impl ClaimLedger {
    /// Starts an empty transcript bound to the exact immutable scope.
    pub fn new(scope: StateScope) -> ContentResult<Self> {
        if scope.table() != super::StateTable::BindingClaims {
            return Err(ContentError::InvalidOrderingRecord("claim table"));
        }
        let mut digest = blake3::Hasher::new();
        digest.update(DOMAIN);
        digest.update(&scope.as_bytes());
        Ok(Self {
            scope,
            digest,
            records: 0,
            encoded_bytes: 0,
            last: None,
        })
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
    pub const fn last(&self) -> Option<ClaimKey> {
        self.last
    }

    /// All widths/order/count/bytes and terminal arithmetic precede append effects.
    pub fn validate_append(&self, records: &[ClaimRecord]) -> ContentResult<()> {
        if records.len() > STATE_MAX_PAGE_RECORDS {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_claims.append_records",
                limit: STATE_MAX_PAGE_RECORDS as u64,
                actual: records.len() as u64,
            });
        }
        let bytes = records
            .len()
            .checked_mul(CLAIM_RECORD_BYTES)
            .and_then(|bytes| bytes.checked_add(CLAIM_APPEND_HEADER_BYTES))
            .ok_or(ContentError::LengthOverflow)?;
        if bytes > STATE_MAX_PAGE_BYTES {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_claims.append_bytes",
                limit: STATE_MAX_PAGE_BYTES as u64,
                actual: bytes as u64,
            });
        }
        let mut last = self.last;
        for record in records {
            ClaimKey::decode(&self.scope, record.key().as_bytes())?;
            if last.is_some_and(|key| key >= record.key()) {
                return Err(ContentError::InvalidOrderingRecord("claim append order"));
            }
            last = Some(record.key());
        }
        self.records
            .checked_add(records.len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        self.encoded_bytes
            .checked_add((bytes - CLAIM_APPEND_HEADER_BYTES) as u64)
            .ok_or(ContentError::LengthOverflow)?;
        Ok(())
    }

    /// Advances digest/totals once the whole checked ordered batch is acknowledged.
    pub fn acknowledge(&mut self, records: &[ClaimRecord]) -> ContentResult<()> {
        self.validate_append(records)?;
        for record in records {
            self.digest.update(&record.encode());
            self.last = Some(record.key());
        }
        self.records += records.len() as u64;
        self.encoded_bytes += (records.len() * CLAIM_RECORD_BYTES) as u64;
        Ok(())
    }

    /// Encodes a checked version1/scope81/count2/record-bytes4 batch header.
    pub fn append_header(
        &self,
        records: &[ClaimRecord],
    ) -> ContentResult<[u8; CLAIM_APPEND_HEADER_BYTES]> {
        self.validate_append(records)?;
        let mut bytes = [0; CLAIM_APPEND_HEADER_BYTES];
        bytes[0] = 1;
        bytes[1..82].copy_from_slice(&self.scope.as_bytes());
        bytes[82..84].copy_from_slice(&(records.len() as u16).to_be_bytes());
        bytes[84..].copy_from_slice(&((records.len() * CLAIM_RECORD_BYTES) as u32).to_be_bytes());
        Ok(bytes)
    }

    /// Snapshots the exact digest with terminal record and byte totals.
    pub fn seal(&self) -> ClaimSeal {
        let mut digest = self.digest.clone();
        digest.update(&self.records.to_be_bytes());
        digest.update(&self.encoded_bytes.to_be_bytes());
        ClaimSeal {
            scope: self.scope.clone(),
            records: self.records,
            encoded_bytes: self.encoded_bytes,
            digest: *digest.finalize().as_bytes(),
        }
    }
}
