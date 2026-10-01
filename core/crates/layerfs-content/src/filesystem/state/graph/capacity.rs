//! Explicit selected disk arithmetic, separate from actual provider fit.
use crate::error::{ContentError, ContentResult};
/// Default and minimum admitted indexed construction-state native bytes.
pub const DEFAULT_GRAPH_SCRATCH_BYTES: u64 = 16 * 1024 * 1024;
/// Largest aligned format budget; representability is not qualification.
pub const MAXIMUM_GRAPH_SCRATCH_BYTES: u64 = (1_u64 << 40) - 4096;
/// One captured disk class; all graph tables share its count/byte allowances.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphCapacity {
    scratch: u64,
    records: u64,
    bytes: u64,
    pages: u64,
}
impl Default for GraphCapacity {
    fn default() -> Self {
        Self {
            scratch: DEFAULT_GRAPH_SCRATCH_BYTES,
            records: 65536,
            bytes: 4128768,
            pages: 4096,
        }
    }
}
impl GraphCapacity {
    /// Validates without rounding, file effects or dependency on current usage.
    pub fn new(scratch: u64) -> ContentResult<Self> {
        if !(DEFAULT_GRAPH_SCRATCH_BYTES..=MAXIMUM_GRAPH_SCRATCH_BYTES).contains(&scratch)
            || scratch % 4096 != 0
        {
            return Err(ContentError::InvalidOrderingRecord("graph scratch budget"));
        }
        let records = scratch / 256;
        if records > u64::from(u32::MAX) {
            return Err(ContentError::LengthOverflow);
        }
        Ok(Self {
            scratch,
            records,
            bytes: records
                .checked_mul(63)
                .ok_or(ContentError::LengthOverflow)?,
            pages: scratch / 4096,
        })
    }
    /// Actual selected native file reservation, not payload capacity.
    pub const fn scratch_bytes(self) -> u64 {
        self.scratch
    }
    /// Aggregate distinct nodes plus edges, not a per-table ceiling.
    pub const fn records(self) -> u64 {
        self.records
    }
    /// Aggregate complete current Node60 and Edge43 bytes.
    pub const fn encoded_bytes(self) -> u64 {
        self.bytes
    }
    /// Selected SQLite page maximum at4096-byte pages.
    pub const fn max_pages(self) -> u64 {
        self.pages
    }
    /// Checked prospective distinct growth before its transaction effects.
    pub fn check_growth(
        self,
        nodes: u64,
        edges: u64,
        added_nodes: u64,
        added_edges: u64,
    ) -> ContentResult<()> {
        let n = nodes
            .checked_add(added_nodes)
            .ok_or(ContentError::LengthOverflow)?;
        let e = edges
            .checked_add(added_edges)
            .ok_or(ContentError::LengthOverflow)?;
        let records = n.checked_add(e).ok_or(ContentError::LengthOverflow)?;
        let bytes = n
            .checked_mul(60)
            .and_then(|a| e.checked_mul(43).and_then(|b| a.checked_add(b)))
            .ok_or(ContentError::LengthOverflow)?;
        if records > self.records {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "graph.records",
                limit: self.records,
                actual: records,
            });
        }
        if bytes > self.bytes {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "graph.record_bytes",
                limit: self.bytes,
                actual: bytes,
            });
        }
        Ok(())
    }
}
