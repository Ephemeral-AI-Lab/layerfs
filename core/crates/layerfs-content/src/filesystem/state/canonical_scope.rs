//! Exact authenticated selected namespace context for counts and release epochs.
use super::{FactSubject, StateScope, StateSelection, StateTable};
use crate::{ContentError, ContentResult};
/// StateScope81 plus the already authenticated FactSubject147.
pub const CANONICAL_SCOPE_BYTES: usize = 228;
/// Closed count/zero/release table role under one authenticated namespace selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalScope {
    state: StateScope,
    subject: FactSubject,
}
impl CanonicalScope {
    /// Table19/phase6 mutable reference effects and exact declared-new membership.
    pub fn counts(selection: StateSelection, subject: FactSubject) -> ContentResult<Self> {
        Ok(Self {
            state: StateScope::new(selection, 6, StateTable::Counts)?,
            subject,
        })
    }
    fn sibling(&self, phase: u64, table: StateTable) -> ContentResult<Self> {
        Ok(Self {
            state: StateScope::new(self.state.selection().clone(), phase, table)?,
            subject: self.subject.clone(),
        })
    }
    /// Exact ordered zero candidates under the same count epoch, table20/phase6.
    pub fn zeros(&self) -> ContentResult<Self> {
        self.sibling(6, StateTable::ZeroSeeds)
    }
    /// Exact FIFO pending jobs, table21/phase7.
    pub fn jobs(&self) -> ContentResult<Self> {
        self.sibling(7, StateTable::ReleaseJobs)
    }
    /// Exact LIFO directory cursor frames, table22/phase7.
    pub fn frames(&self) -> ContentResult<Self> {
        self.sibling(7, StateTable::ReleaseFrames)
    }
    /// Complete issued owner/phase/table association.
    pub fn state(&self) -> &StateScope {
        &self.state
    }
    /// Original subject and actual immutable inode table.
    pub fn subject(&self) -> &FactSubject {
        &self.subject
    }
    /// Full selected context without hash truncation.
    pub fn encode(&self) -> [u8; CANONICAL_SCOPE_BYTES] {
        let mut b = [0; CANONICAL_SCOPE_BYTES];
        b[..81].copy_from_slice(&self.state.as_bytes());
        b[81..].copy_from_slice(&self.subject.encode());
        b
    }
    /// Scoped positive scalar/rank, preserving all64 bits within SQLite's signed domain.
    pub fn key(&self, scalar: u64) -> ContentResult<[u8; 25]> {
        if scalar == 0 || scalar > i64::MAX as u64 {
            return Err(ContentError::InvalidOrderingRecord("canonical scalar"));
        }
        let mut key = [0; 25];
        key[..8].copy_from_slice(&self.state.selection().token().to_be_bytes());
        key[8..16].copy_from_slice(&self.state.phase().to_be_bytes());
        key[16] = self.state.table().code();
        key[17..].copy_from_slice(&scalar.to_be_bytes());
        Ok(key)
    }
    /// Validate complete selected key framing before exposing the scalar.
    pub fn scalar(&self, key: &[u8]) -> ContentResult<u64> {
        if key.len() != 25 {
            return Err(ContentError::InvalidOrderingRecord("canonical key width"));
        }
        let scalar = u64::from_be_bytes(key[17..].try_into().unwrap());
        if key != self.key(scalar)? {
            return Err(ContentError::InvalidOrderingRecord(
                "canonical selected key",
            ));
        }
        Ok(scalar)
    }
}
