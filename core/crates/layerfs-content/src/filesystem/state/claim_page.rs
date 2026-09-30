//! Bounded exact sealed pages; wire widths are distinct from native ownership.
use crate::error::{ContentError, ContentResult};

use super::{
    claim_records::CLAIM_RECORD_BYTES, ClaimKey, ClaimRecord, ClaimSeal, STATE_MAX_PAGE_BYTES,
    STATE_MAX_PAGE_RECORDS,
};

/// seal130/last-present1/last-key25/count2/record-bytes4/EOF1.
pub const CLAIM_PAGE_HEADER_BYTES: usize = 163;

/// Simultaneous record-count and encoded-byte bounds for one output page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaimPageLimit {
    records: usize,
    bytes: usize,
}

impl Default for ClaimPageLimit {
    fn default() -> Self {
        Self {
            records: STATE_MAX_PAGE_RECORDS,
            bytes: STATE_MAX_PAGE_BYTES,
        }
    }
}

impl ClaimPageLimit {
    /// Checks nonzero count and byte limits against the implemented profile.
    pub fn new(records: usize, bytes: usize) -> ContentResult<Self> {
        if records == 0
            || records > STATE_MAX_PAGE_RECORDS
            || !(CLAIM_PAGE_HEADER_BYTES..=STATE_MAX_PAGE_BYTES).contains(&bytes)
        {
            return Err(ContentError::InvalidOrderingRecord("claim page limit"));
        }
        Ok(Self { records, bytes })
    }

    /// Maximum returned records and retained native record capacity.
    pub const fn records(self) -> usize {
        self.records
    }
    /// Maximum encoded page bytes, including the complete page header.
    pub const fn bytes(self) -> usize {
        self.bytes
    }

    /// Refuses a count whose records or header-inclusive encoding exceed a bound.
    pub fn check_records(self, records: usize) -> ContentResult<()> {
        if records > self.records {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_claims.page_records",
                limit: self.records as u64,
                actual: records as u64,
            });
        }
        let bytes = records
            .checked_mul(CLAIM_RECORD_BYTES)
            .and_then(|bytes| bytes.checked_add(CLAIM_PAGE_HEADER_BYTES))
            .ok_or(ContentError::LengthOverflow)?;
        if bytes > self.bytes {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_claims.page_bytes",
                limit: self.bytes as u64,
                actual: bytes as u64,
            });
        }
        Ok(())
    }

    /// Rows that fit both bounds, including the fixed header.
    pub fn fitting_records(self) -> usize {
        self.records
            .min((self.bytes - CLAIM_PAGE_HEADER_BYTES) / CLAIM_RECORD_BYTES)
    }
}

/// A single checked selected page; constructor reserves no hidden population.
#[derive(Debug)]
pub struct ClaimPage {
    seal: ClaimSeal,
    records: Vec<ClaimRecord>,
    last: Option<ClaimKey>,
    eof: bool,
}

impl ClaimPage {
    /// Checks a first page's keys, widths, capacity and progress.
    pub fn new(seal: ClaimSeal, records: Vec<ClaimRecord>, eof: bool) -> ContentResult<Self> {
        Self::after(seal, None, records, eof)
    }

    /// Checks a continuation against its actual prior key. Empty EOF preserves it.
    /// The consuming cursor independently verifies transcript counts and digest.
    pub fn after(
        seal: ClaimSeal,
        after: Option<ClaimKey>,
        records: Vec<ClaimRecord>,
        eof: bool,
    ) -> ContentResult<Self> {
        ClaimPageLimit::default().check_records(records.len())?;
        if records.capacity() > STATE_MAX_PAGE_RECORDS {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_claims.page_capacity",
                limit: STATE_MAX_PAGE_RECORDS as u64,
                actual: records.capacity() as u64,
            });
        }
        if records.is_empty() && !eof {
            return Err(ContentError::InvalidOrderingRecord(
                "claim empty continuation",
            ));
        }
        let mut last = after;
        if let Some(key) = after {
            ClaimKey::decode(seal.scope(), key.as_bytes())?;
        }
        for record in &records {
            ClaimKey::decode(seal.scope(), record.key().as_bytes())?;
            if last.is_some_and(|key| key >= record.key()) {
                return Err(ContentError::InvalidOrderingRecord("claim page order"));
            }
            last = Some(record.key());
        }
        Ok(Self {
            seal,
            records,
            last,
            eof,
        })
    }

    /// The provider's exact selected seal for this page.
    pub fn seal(&self) -> &ClaimSeal {
        &self.seal
    }
    /// The complete checked returned records in strict key order.
    pub fn records(&self) -> &[ClaimRecord] {
        &self.records
    }
    /// Transfers the current page's actual record allocation to its consumer.
    pub fn into_records(self) -> Vec<ClaimRecord> {
        self.records
    }
    /// The provider's terminal-count assertion, checked by the consuming cursor.
    pub const fn eof(&self) -> bool {
        self.eof
    }
    /// Actual last emitted key, or prior/none on an empty terminal page.
    pub const fn last(&self) -> Option<ClaimKey> {
        self.last
    }
    /// Exact header-inclusive encoded size, distinct from native allocation size.
    pub fn encoded_len(&self) -> usize {
        CLAIM_PAGE_HEADER_BYTES + self.records.len() * CLAIM_RECORD_BYTES
    }

    /// Checks the caller's smaller bounds, including actual native Vec capacity.
    pub fn check_limit(&self, limit: ClaimPageLimit) -> ContentResult<()> {
        limit.check_records(self.records.len())?;
        if self.records.capacity() > limit.records() {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_claims.page_capacity",
                limit: limit.records() as u64,
                actual: self.records.capacity() as u64,
            });
        }
        Ok(())
    }

    /// Encodes seal130/presence1/key25/count2/record-bytes4/EOF1 without allocation.
    pub fn encode_header(&self) -> [u8; CLAIM_PAGE_HEADER_BYTES] {
        let mut bytes = [0; CLAIM_PAGE_HEADER_BYTES];
        bytes[..130].copy_from_slice(&self.seal.encode());
        if let Some(last) = self.last {
            bytes[130] = 1;
            bytes[131..156].copy_from_slice(last.as_bytes());
        }
        bytes[156..158].copy_from_slice(&(self.records.len() as u16).to_be_bytes());
        bytes[158..162]
            .copy_from_slice(&((self.records.len() * CLAIM_RECORD_BYTES) as u32).to_be_bytes());
        bytes[162] = u8::from(self.eof);
        bytes
    }
}
