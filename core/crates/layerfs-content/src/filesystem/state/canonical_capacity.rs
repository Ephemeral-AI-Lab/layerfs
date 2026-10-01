//! One prospective same-file canonical population class, independent of GraphR.
use super::{FactOccupancy, ALIAS_FIXED_BYTES, FACT_OWNER_ALLOWANCE};
use crate::{ContentError, ContentResult};
/// Fixed logical canonical metadata allowance, distinct from physical SQL sizes.
pub const CANONICAL_OWNER_ALLOWANCE: u64 = 2048;
/// Complete scoped Count128/Zero105/Job113/Frame322 widths.
pub const COUNT_RECORD_BYTES: u64 = 128;
/// Exact zero seed record width.
pub const ZERO_SEED_BYTES: u64 = 105;
/// Exact FIFO pending record width.
pub const RELEASE_JOB_BYTES: u64 = 113;
/// Exact full-name stack frame record width.
pub const RELEASE_FRAME_BYTES: u64 = 322;
/// Independently captured record classes and one shared aggregate byte ceiling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalCapacity {
    counts: u64,
    zeros: u64,
    jobs: u64,
    frames: u64,
    bytes: u64,
}
impl CanonicalCapacity {
    /// Capture exact logical classes before dependent SQL or allocation.
    pub fn new(counts: u64, zeros: u64, jobs: u64, frames: u64, bytes: u64) -> ContentResult<Self> {
        if counts == 0
            || [counts, zeros, jobs, frames]
                .into_iter()
                .any(|n| n > i64::MAX as u64)
            || bytes < ALIAS_FIXED_BYTES + FACT_OWNER_ALLOWANCE + CANONICAL_OWNER_ALLOWANCE
        {
            return Err(ContentError::InvalidOrderingRecord("canonical capacity"));
        }
        Ok(Self {
            counts,
            zeros,
            jobs,
            frames,
            bytes,
        })
    }
    /// Exact all-zero class for a separately verified bounded nonnative source.
    pub const fn verified_empty() -> Self {
        Self {
            counts: 0,
            zeros: 0,
            jobs: 0,
            frames: 0,
            bytes: ALIAS_FIXED_BYTES + FACT_OWNER_ALLOWANCE + CANONICAL_OWNER_ALLOWANCE,
        }
    }
    /// Count/declared-new row limit.
    pub const fn counts(self) -> u64 {
        self.counts
    }
    /// Zero-candidate row limit.
    pub const fn zeros(self) -> u64 {
        self.zeros
    }
    /// Simultaneous FIFO pending row limit.
    pub const fn jobs(self) -> u64 {
        self.jobs
    }
    /// Simultaneous native cursor depth limit.
    pub const fn frames(self) -> u64 {
        self.frames
    }
    /// Shared aggregate current-record byte limit.
    pub const fn bytes(self) -> u64 {
        self.bytes
    }
    /// Captured40-byte private profile8 native-binding suffix.
    pub fn encode(self) -> [u8; 40] {
        let mut b = [0; 40];
        for (i, n) in [self.counts, self.zeros, self.jobs, self.frames, self.bytes]
            .into_iter()
            .enumerate()
        {
            b[i * 8..i * 8 + 8].copy_from_slice(&n.to_be_bytes());
        }
        b
    }
    /// Complete simultaneous population admission before any dependent mutation.
    pub fn check(self, live: CanonicalOccupancy) -> ContentResult<()> {
        for (actual, limit, what) in [
            (live.counts, self.counts, "counts.records"),
            (live.zeros, self.zeros, "zero_seeds.records"),
            (live.jobs, self.jobs, "release.jobs"),
            (live.frames, self.frames, "release.frames"),
        ] {
            if actual > limit {
                return Err(ContentError::BoundedCapacityExceeded {
                    what,
                    limit,
                    actual,
                });
            }
        }
        let n = live.namespace;
        let populations = [
            (n.facts, 105),
            (n.parents, 32),
            (n.sites, 60),
            (n.alias_facts, 40),
            (n.alias_jobs, 39),
            (n.graph_nodes, 60),
            (n.graph_edges, 43),
            (n.roots, 63),
            (live.counts, 128),
            (live.zeros, 105),
            (live.jobs, 113),
            (live.frames, 322),
        ];
        let mut bytes = ALIAS_FIXED_BYTES + FACT_OWNER_ALLOWANCE + CANONICAL_OWNER_ALLOWANCE;
        for (records, width) in populations {
            bytes = records
                .checked_mul(width)
                .and_then(|r| bytes.checked_add(r))
                .ok_or(ContentError::LengthOverflow)?;
        }
        if bytes > self.bytes {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "canonical.aggregate_bytes",
                limit: self.bytes,
                actual: bytes,
            });
        }
        Ok(())
    }
}
/// Acknowledged live rows of all simultaneously retained namespace roles.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CanonicalOccupancy {
    /// Actual profile7 namespace populations, not lifetime totals.
    pub namespace: FactOccupancy,
    /// Mutable or sealed Count rows.
    pub counts: u64,
    /// Live zero-candidate rows.
    pub zeros: u64,
    /// Pending jobs plus the current owned job.
    pub jobs: u64,
    /// Live cursor frames.
    pub frames: u64,
}
