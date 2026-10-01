//! Present/absent versus unknown facts and same-file logical aggregate admission.
use super::ALIAS_FIXED_BYTES;
use crate::object::inode_leaf::{decode_inode_value, encode_inode_value, InodeValue};
use crate::{ContentError, ContentResult};
/// Complete key25/presence1/inode73/frame6 width.
pub const BASE_FACT_BYTES: u64 = 105;
/// Complete key25/bound1/frame6 width.
pub const PARENT_ELIGIBILITY_BYTES: u64 = 32;
/// Fixed prospective metadata allowance within S, separate from physical allocation.
pub const FACT_OWNER_ALLOWANCE: u64 = 1024;
/// One immutable answer. No row is unknown; value=None is established absence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BaseFact {
    /// Positive selected inode serial.
    pub serial: u64,
    /// Some is an authenticated value; None is authenticated absence.
    pub value: Option<InodeValue>,
}
impl BaseFact {
    /// Canonical presence/value payload with zero absent fields.
    pub fn encode_value(self) -> ContentResult<[u8; 74]> {
        if self.serial == 0 || self.serial > i64::MAX as u64 {
            return Err(ContentError::InvalidOrderingRecord("fact serial"));
        }
        let mut value = [0; 74];
        if let Some(inode) = self.value {
            value[0] = 1;
            value[1..].copy_from_slice(&encode_inode_value(inode));
        }
        Ok(value)
    }
    /// Decode an exact payload without treating an unknown key as absence.
    pub fn decode(serial: u64, bytes: &[u8]) -> ContentResult<Self> {
        if bytes.len() != 74 || bytes[0] > 1 {
            return Err(ContentError::InvalidOrderingRecord("base fact framing"));
        }
        let value = if bytes[0] == 1 {
            Some(decode_inode_value(bytes[1..].try_into().unwrap())?)
        } else {
            if bytes[1..] != [0; 73] {
                return Err(ContentError::InvalidOrderingRecord("absent base fact"));
            }
            None
        };
        let fact = Self { serial, value };
        fact.encode_value()?;
        Ok(fact)
    }
}
/// One declared-new nonroot directory and its monotone incoming-binding fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParentFact {
    /// Positive declared directory serial.
    pub serial: u64,
    /// Whether at least one final scalar binding names it.
    pub bound: bool,
}
/// Independent fact/parent limits; graphR is not their allowance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactCapacity {
    facts: u64,
    parents: u64,
    bytes: u64,
}
impl FactCapacity {
    /// Exact all-zero logical class for a separately verified nonnative authority.
    /// Fixed framing remains accounted, with no native file allocation claim.
    pub const fn verified_empty() -> Self {
        Self {
            facts: 0,
            parents: 0,
            bytes: FACT_OWNER_ALLOWANCE + ALIAS_FIXED_BYTES,
        }
    }
    /// Capture independent records/aggregate limits before dependent effects.
    pub fn new(facts: u64, parents: u64, aggregate_bytes: u64) -> ContentResult<Self> {
        if facts == 0
            || facts > i64::MAX as u64
            || parents > i64::MAX as u64
            || aggregate_bytes < FACT_OWNER_ALLOWANCE + ALIAS_FIXED_BYTES
        {
            return Err(ContentError::InvalidOrderingRecord("fact capacity"));
        }
        Ok(Self {
            facts,
            parents,
            bytes: aggregate_bytes,
        })
    }
    /// Selected BaseFacts record allowance.
    pub const fn facts(self) -> u64 {
        self.facts
    }
    /// Selected ParentEligibility record allowance.
    pub const fn parents(self) -> u64 {
        self.parents
    }
    /// Combined same-file logical byte ceiling.
    pub const fn bytes(self) -> u64 {
        self.bytes
    }
    /// Fixed24-byte captured class for private native binding.
    pub fn encode(self) -> [u8; 24] {
        let mut b = [0; 24];
        b[..8].copy_from_slice(&self.facts.to_be_bytes());
        b[8..16].copy_from_slice(&self.parents.to_be_bytes());
        b[16..].copy_from_slice(&self.bytes.to_be_bytes());
        b
    }
    /// Actual simultaneous populations, before the dependent mutation.
    pub fn check(self, live: FactOccupancy) -> ContentResult<()> {
        if live.parents > self.parents {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "parents.records",
                limit: self.parents,
                actual: live.parents,
            });
        }
        if live.facts > self.facts {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "facts.records",
                limit: self.facts,
                actual: live.facts,
            });
        }
        let populations = [
            (live.facts, BASE_FACT_BYTES),
            (live.parents, PARENT_ELIGIBILITY_BYTES),
            (live.sites, 60),
            (live.alias_facts, 40),
            (live.alias_jobs, 39),
            (live.graph_nodes, 60),
            (live.graph_edges, 43),
            (live.roots, 63),
        ];
        let mut bytes = FACT_OWNER_ALLOWANCE + ALIAS_FIXED_BYTES;
        for (records, width) in populations {
            bytes = records
                .checked_mul(width)
                .and_then(|n| bytes.checked_add(n))
                .ok_or(ContentError::LengthOverflow)?;
        }
        if bytes > self.bytes {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "facts.aggregate_bytes",
                limit: self.bytes,
                actual: bytes,
            });
        }
        Ok(())
    }
}
/// Concrete simultaneous namespace scratch populations, with no independent budgets.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FactOccupancy {
    /// Live BaseFacts rows.
    pub facts: u64,
    /// Live ParentEligibility rows.
    pub parents: u64,
    /// Still-owned Sites rows.
    pub sites: u64,
    /// Live AliasFacts rows.
    pub alias_facts: u64,
    /// Pending AliasJobs rows.
    pub alias_jobs: u64,
    /// Live GraphNodes rows.
    pub graph_nodes: u64,
    /// Live GraphEdges rows.
    pub graph_edges: u64,
    /// Live DirectoryRoots rows.
    pub roots: u64,
}
