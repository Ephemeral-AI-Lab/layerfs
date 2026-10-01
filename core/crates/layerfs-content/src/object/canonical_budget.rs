//! Exact returned/copied canonical Vec capacity credits, not a process budget.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use crate::error::{ContentError, ContentResult};

/// Existing larger returned-canonical compatibility data-capacity class.
pub const CANONICAL_COMPATIBILITY_BYTES: usize = 32 * 1024 * 1024;

struct Shared {
    limit: usize,
    used: AtomicUsize,
}
/// One bounded canonical-data owner shared by returned results and their copies.
/// Descriptors, decoded structures, provider/native/cache work are separate.
#[derive(Clone)]
pub struct CanonicalBudget(Arc<Shared>);
impl CanonicalBudget {
    /// Select exactly `limit` data-capacity bytes before provider/copy effects.
    /// A limit above the existing32MiB compatibility maximum is refused.
    pub fn new(limit: usize) -> ContentResult<Self> {
        if limit > CANONICAL_COMPATIBILITY_BYTES {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "canonical.budget_limit",
                limit: CANONICAL_COMPATIBILITY_BYTES as u64,
                actual: limit as u64,
            });
        }
        Ok(Self::fixed(limit))
    }
    fn fixed(limit: usize) -> Self {
        Self(Arc::new(Shared {
            limit,
            used: AtomicUsize::new(0),
        }))
    }
    /// Existing32MiB compatibility class; no full payload reservation while idle.
    pub fn compatibility() -> Self {
        Self::fixed(CANONICAL_COMPATIBILITY_BYTES)
    }
    /// Captured exact data-capacity limit.
    pub fn limit(&self) -> usize {
        self.0.limit
    }
    /// Live returned/copied Vec capacity still held by actual data owners.
    pub fn used_bytes(&self) -> usize {
        self.0.used.load(Ordering::Acquire)
    }
    /// Continuing budget authority identity, independent of an equal numeric limit.
    pub fn same_authority(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    /// Admit prospective capacity before acquiring the allocation it will fund.
    pub fn reserve(&self, bytes: usize) -> ContentResult<CanonicalLease> {
        self.0
            .used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(bytes)
                    .filter(|total| *total <= self.0.limit)
            })
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "canonical.data_capacity",
            })?;
        Ok(CanonicalLease {
            budget: self.clone(),
            bytes,
        })
    }
}
impl Default for CanonicalBudget {
    fn default() -> Self {
        Self::compatibility()
    }
}

/// Exclusive prospective capacity credit. Moving it never refunds its data.
pub struct CanonicalLease {
    budget: CanonicalBudget,
    bytes: usize,
}
impl CanonicalLease {
    /// Exact credit retained by this owner.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    /// Shared authority for a separately admitted immutable copy.
    pub fn budget(&self) -> &CanonicalBudget {
        &self.budget
    }
    /// Move a portion into a separately owned actual allocation.
    pub fn split(&mut self, bytes: usize) -> ContentResult<Self> {
        if bytes > self.bytes {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "canonical.permit_capacity",
                limit: self.bytes as u64,
                actual: bytes as u64,
            });
        }
        self.bytes -= bytes;
        Ok(Self {
            budget: self.budget.clone(),
            bytes,
        })
    }
}
impl Drop for CanonicalLease {
    fn drop(&mut self) {
        self.budget.0.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
