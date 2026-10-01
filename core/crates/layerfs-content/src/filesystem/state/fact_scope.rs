//! Exact authenticated selected inode table, never inferred from a source token.
use super::{GraphSubject, StateScope, StateSelection, StateTable};
use crate::filesystem::inode::read::InodeTable;
use crate::{ContentError, ContentResult, ObjectId};
/// GraphSubject106/table-present1/table-root32/table-root-serial8.
pub const FACT_SUBJECT_BYTES: usize = 147;
/// Exact state scope81 plus authenticated subject147.
pub const FACT_SCOPE_BYTES: usize = 228;
/// Immutable selection plus the actual checked canonical inode table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactSubject {
    selected: GraphSubject,
    table: Option<InodeTable>,
}
impl FactSubject {
    /// C1 supplies this only after authenticated selected filesystem-root validation.
    pub fn new(selected: GraphSubject, table: Option<InodeTable>) -> ContentResult<Self> {
        if selected.base().is_some() != table.is_some()
            || table.is_some_and(|t| t.root_serial != selected.root_serial())
        {
            return Err(ContentError::InvalidOrderingRecord(
                "fact selected inode table",
            ));
        }
        Ok(Self { selected, table })
    }
    /// Original exact SourceId/namespace/Base/root association.
    pub fn selected(&self) -> &GraphSubject {
        &self.selected
    }
    /// Actual immutable authenticated table, absent for a fresh operation.
    pub const fn table(&self) -> Option<InodeTable> {
        self.table
    }
    /// Canonical fixed subject, including explicit absent table placeholders.
    pub fn encode(&self) -> [u8; FACT_SUBJECT_BYTES] {
        let mut bytes = [0; FACT_SUBJECT_BYTES];
        bytes[..106].copy_from_slice(&self.selected.encode());
        if let Some(table) = self.table {
            bytes[106] = 1;
            bytes[107..139].copy_from_slice(table.root.as_bytes());
            bytes[139..].copy_from_slice(&table.root_serial.to_be_bytes());
        }
        bytes
    }
    /// Requires the original issued subject; bare source bytes cannot select it.
    pub fn decode(selected: &GraphSubject, bytes: &[u8]) -> ContentResult<Self> {
        if bytes.len() != FACT_SUBJECT_BYTES || bytes[..106] != selected.encode() || bytes[106] > 1
        {
            return Err(ContentError::InvalidOrderingRecord("fact subject framing"));
        }
        let table = if bytes[106] == 1 {
            Some(InodeTable {
                root: ObjectId::from_bytes(&bytes[107..139])?,
                root_serial: u64::from_be_bytes(bytes[139..].try_into().unwrap()),
            })
        } else {
            if bytes[107..] != [0; 40] {
                return Err(ContentError::InvalidOrderingRecord("fact absent table"));
            }
            None
        };
        Self::new(selected.clone(), table)
    }
}
/// Exact table/epoch and selected immutable base relation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactScope {
    state: StateScope,
    subject: FactSubject,
}
impl FactScope {
    /// Issued BaseFacts table17 phase4 association.
    pub fn new(selection: StateSelection, subject: FactSubject) -> ContentResult<Self> {
        Ok(Self {
            state: StateScope::new(selection, 4, StateTable::BaseFacts)?,
            subject,
        })
    }
    /// Issued ParentEligibility table18 phase5 association.
    pub fn parents(selection: StateSelection, subject: FactSubject) -> ContentResult<Self> {
        Ok(Self {
            state: StateScope::new(selection, 5, StateTable::ParentEligibility)?,
            subject,
        })
    }
    /// Full issued native selection and exact phase/table.
    pub fn state(&self) -> &StateScope {
        &self.state
    }
    /// Exact selected canonical base/table subject.
    pub fn subject(&self) -> &FactSubject {
        &self.subject
    }
    /// Complete228-byte private scope.
    pub fn encode(&self) -> [u8; FACT_SCOPE_BYTES] {
        let mut bytes = [0; FACT_SCOPE_BYTES];
        bytes[..81].copy_from_slice(&self.state.as_bytes());
        bytes[81..].copy_from_slice(&self.subject.encode());
        bytes
    }
    /// Exact scoped scalar key, with no serial truncation.
    pub fn key(&self, serial: u64) -> ContentResult<[u8; 25]> {
        if serial == 0 || serial > i64::MAX as u64 {
            return Err(ContentError::InvalidOrderingRecord("fact serial"));
        }
        let mut key = [0; 25];
        key[..8].copy_from_slice(&self.state.selection().token().to_be_bytes());
        key[8..16].copy_from_slice(&self.state.phase().to_be_bytes());
        key[16] = self.state.table().code();
        key[17..].copy_from_slice(&serial.to_be_bytes());
        Ok(key)
    }
    /// Verify key framing and selected epoch/table before returning its serial.
    pub fn serial(&self, key: &[u8]) -> ContentResult<u64> {
        if key.len() != 25 {
            return Err(ContentError::InvalidOrderingRecord("fact key framing"));
        }
        let serial = u64::from_be_bytes(key[17..].try_into().unwrap());
        if key != self.key(serial)? {
            return Err(ContentError::InvalidOrderingRecord("fact selected key"));
        }
        Ok(serial)
    }
}
