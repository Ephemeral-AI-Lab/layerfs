//! Bounded advisory predecessors with explicit provenance.
//!
//! A predecessor is a hint, never a dependency: C1 declares which stored objects
//! it believes a new object resembles, in preference order, and C2 decides
//! whether any of them is acquired and worth encoding against. The list is
//! bounded and carries why each identity was proposed, so a caller can tell a
//! declared base apart from a shifted-coordinate or subtree hint. No codec, SQL
//! connection, host role or mutable handle belongs here.

use crate::error::{ContentError, ContentResult};
use crate::object::ObjectId;

/// Largest number of advisory predecessors one object may carry.
pub const MAXIMUM_ADVISORY_PREDECESSORS: usize = 4;

/// Why an advisory predecessor was proposed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredecessorProvenance {
    /// The base object this operation was declared to edit.
    OriginalBase,
    /// The object that held bytes immediately before the replaced range.
    UnchangedPrefix,
    /// A stored object covering a reused subtree or extent range.
    ReusedRange,
}

/// One advisory predecessor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdvisoryPredecessor {
    id: ObjectId,
    provenance: PredecessorProvenance,
}

impl AdvisoryPredecessor {
    /// Identity proposed as a base.
    pub const fn id(self) -> ObjectId {
        self.id
    }

    /// Why it was proposed.
    pub const fn provenance(self) -> PredecessorProvenance {
        self.provenance
    }
}

/// Ordered, bounded advisory predecessor list.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AdvisoryPredecessors {
    entries: Vec<AdvisoryPredecessor>,
}

impl AdvisoryPredecessors {
    /// Empty list.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Appends one predecessor, rejecting a list beyond the declared bound.
    pub fn push(&mut self, id: ObjectId, provenance: PredecessorProvenance) -> ContentResult<()> {
        if self.entries.len() >= MAXIMUM_ADVISORY_PREDECESSORS {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "object.predecessors",
                limit: MAXIMUM_ADVISORY_PREDECESSORS as u64,
                actual: self.entries.len() as u64 + 1,
            });
        }
        if self.entries.iter().any(|entry| entry.id == id) {
            return Ok(());
        }
        self.entries.push(AdvisoryPredecessor { id, provenance });
        Ok(())
    }

    /// Number of predecessors held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no predecessor is proposed.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Proposed identities, in preference order.
    pub fn ids(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.entries.iter().map(|entry| entry.id)
    }

    /// Proposed identities as a slice-compatible vector.
    pub fn to_ids(&self) -> Vec<ObjectId> {
        self.ids().collect()
    }

    /// Entries with their provenance, in preference order.
    pub fn entries(&self) -> &[AdvisoryPredecessor] {
        &self.entries
    }

    /// Builds a list from one explicit predecessor.
    pub fn explicit(id: ObjectId) -> ContentResult<Self> {
        let mut list = Self::new();
        list.push(id, PredecessorProvenance::OriginalBase)?;
        Ok(list)
    }
}
