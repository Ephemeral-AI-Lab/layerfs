//! Exact discovery records and independently admitted simultaneous frontier bytes.
use crate::{ContentError, ContentResult};

/// Full scope89/membership-presence1/membership164/stage1/counters32/current17/
/// progress264/remaining8/last-presence1/last-serial8 fixed owner header.
pub const ALIAS_FIXED_BYTES: u64 = 585;
/// Complete fact key/value/frame width.
pub const ALIAS_FACT_BYTES: u64 = 40;
/// Complete job key/value/frame width.
pub const ALIAS_JOB_BYTES: u64 = 39;
/// One exact current-state fact; jobs are its pending sequence projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AliasFact {
    /// Positive inode serial.
    pub serial: u64,
    /// Last monotonically issued discovery priority.
    pub sequence: u64,
    /// Pending1/current2/expanded3.
    pub status: u8,
}
impl AliasFact {
    /// Refuses malformed scalar/state values before provider effects.
    pub fn check(self) -> ContentResult<()> {
        if self.serial == 0
            || self.serial > i64::MAX as u64
            || self.sequence == 0
            || self.sequence > i64::MAX as u64
            || !(1..=3).contains(&self.status)
        {
            return Err(ContentError::InvalidOrderingRecord("alias fact"));
        }
        Ok(())
    }
}
/// Alias allowance shares one file with a pre-admitted Sites population.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AliasCapacity {
    records: u64,
    aggregate_bytes: u64,
    site_bytes: u64,
}
impl AliasCapacity {
    /// Exact all-zero logical class for a separately verified nonnative authority.
    /// It grants no fact/job growth and declares no native allocation.
    pub const fn verified_empty() -> Self {
        Self {
            records: 0,
            aggregate_bytes: ALIAS_FIXED_BYTES,
            site_bytes: 0,
        }
    }
    /// Captures independent logical limits; makes no SQL/physical fit claim.
    pub fn new(records: u64, aggregate_bytes: u64, sites: u64) -> ContentResult<Self> {
        let site_bytes = sites.checked_mul(60).ok_or(ContentError::LengthOverflow)?;
        if records == 0
            || records > i64::MAX as u64
            || site_bytes
                .checked_add(ALIAS_FIXED_BYTES)
                .is_none_or(|n| n > aggregate_bytes)
        {
            return Err(ContentError::InvalidOrderingRecord("alias capacity"));
        }
        Ok(Self {
            records,
            aggregate_bytes,
            site_bytes,
        })
    }
    /// Exact pre-effect combined Sites/facts/jobs growth check.
    pub fn check(self, facts: u64, jobs: u64) -> ContentResult<()> {
        if facts > self.records || jobs > facts {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "alias.records",
                limit: self.records,
                actual: facts.max(jobs),
            });
        }
        let bytes = facts
            .checked_mul(ALIAS_FACT_BYTES)
            .and_then(|n| {
                jobs.checked_mul(ALIAS_JOB_BYTES)
                    .and_then(|j| n.checked_add(j))
            })
            .and_then(|n| n.checked_add(self.site_bytes))
            .and_then(|n| n.checked_add(ALIAS_FIXED_BYTES))
            .ok_or(ContentError::LengthOverflow)?;
        if bytes > self.aggregate_bytes {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "alias.sites_and_frontier_bytes",
                limit: self.aggregate_bytes,
                actual: bytes,
            });
        }
        Ok(())
    }
    /// Captured record ceiling.
    pub const fn records(self) -> u64 {
        self.records
    }
    /// Combined logical byte allowance.
    pub const fn aggregate_bytes(self) -> u64 {
        self.aggregate_bytes
    }
    /// Reserved declared Sites bytes, not a second full scratch allowance.
    pub const fn site_bytes(self) -> u64 {
        self.site_bytes
    }
}
