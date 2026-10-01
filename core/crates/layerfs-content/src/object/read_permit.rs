//! Prospective count/role/output capacity selected before a supplied read.

use super::{CanonicalBudget, CanonicalLease, ObjectRole};
use crate::error::{ContentError, ContentResult};

/// Immutable caller's canonical logical-role demand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalReadKind {
    /// Independent authenticated provider's existing general canonical demand.
    Any,
    /// Extent leaf/branch navigation pages only.
    MappingNodes,
    /// Chunk payloads only, with the existing canonical chunk envelope.
    ChunkPayloads,
}
impl CanonicalReadKind {
    /// Check actual locator role before payload/pack decoding.
    pub fn accepts(self, role: ObjectRole) -> bool {
        match self {
            Self::Any => true,
            Self::MappingNodes => matches!(role, ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch),
            Self::ChunkPayloads => role == ObjectRole::Chunk,
        }
    }
}

/// Scope of the supplier's returned-buffer ownership guarantee.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalOwnership {
    /// Raw compatibility adapter: upstream allocation is unqualified.
    Compatibility,
    /// Supplier admitted output before decode; lower working/native fit is open.
    AdmittedOutput,
}

/// Exclusive prospective output credit selected before invoking the supplier.
pub struct CanonicalReadPermit {
    count: usize,
    maximum: usize,
    kind: CanonicalReadKind,
    pub(super) lease: CanonicalLease,
}
impl CanonicalReadPermit {
    /// Count and maximum actual output capacity, not decoded payload length.
    pub fn new(
        budget: &CanonicalBudget,
        count: usize,
        maximum: usize,
        kind: CanonicalReadKind,
    ) -> ContentResult<Self> {
        Ok(Self {
            count,
            maximum,
            kind,
            lease: budget.reserve(maximum)?,
        })
    }
    /// Exact demand cardinality selected by the consumer.
    pub fn count(&self) -> usize {
        self.count
    }
    /// Prospective canonical output capacity already funded.
    pub fn maximum_capacity(&self) -> usize {
        self.maximum
    }
    /// Selected immutable logical-role demand.
    pub fn kind(&self) -> CanonicalReadKind {
        self.kind
    }
    /// Check exact demand before any supplier effects.
    pub fn check_count(&self, count: usize) -> ContentResult<()> {
        if count != self.count {
            return Err(ContentError::BatchCardinality {
                requested: self.count,
                returned: count,
            });
        }
        Ok(())
    }
}
