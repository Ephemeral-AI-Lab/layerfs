//! Caller-owned indexed edit records, independent of database/runtime types.
use crate::ContentResult;

/// Complete raw record key; Content owns the meaning of its domain kind.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EditRecordKey {
    /// Record domain, independent of canonical object identity.
    pub kind: u32,
    /// All32bytes of the record identity.
    pub key: [u8; 32],
}
/// Exact guarded pre-state of one record.
#[derive(Debug, Eq, PartialEq)]
pub enum EditRecordExpected {
    /// The target must be absent.
    Missing,
    /// The target must contain these exact original bytes.
    ExactBytes(Vec<u8>),
}
/// One raw change in a strictly ordered unique bounded batch.
#[derive(Debug, Eq, PartialEq)]
pub struct EditRecordChange {
    /// Complete target key.
    pub key: EditRecordKey,
    /// Original expected state, checked before every batch effect.
    pub expected: EditRecordExpected,
    /// Some writes a BLOB (including empty); None deletes.
    pub value: Option<Vec<u8>>,
}
/// Original atomic application outcome; NotApplied permits no replay.
#[derive(Debug, Eq, PartialEq)]
pub enum EditRecordApply {
    /// Every change was applied atomically.
    Applied,
    /// No record changed. The provider retains original underlying custody.
    NotApplied {
        /// Deciding target's position in the original ordered batch.
        index: usize,
        /// Complete deciding key.
        key: EditRecordKey,
        /// Original actual bounded value, or absence.
        actual: Option<Vec<u8>>,
    },
}
/// A single caller-bound operation/file scope. Methods are short bounded jobs;
/// the provider retains the first original refusal/completion and becomes
/// terminal. Scope acquisition/release and actual consumer fences belong to the
/// caller. Drop performs no release, retry or guessed cleanup.
pub trait IndexedEditBacking {
    /// Metadata-only exact membership.
    fn contains(&mut self, key: EditRecordKey) -> ContentResult<bool>;
    /// One bounded original raw value.
    fn get(&mut self, key: EditRecordKey) -> ContentResult<Option<Vec<u8>>>;
    /// One sorted unique guarded batch within a64KiB retained-capacity envelope.
    fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<EditRecordApply>;
    /// First at most64 keys, excluding only a root in this exact domain. None
    /// excludes nothing; no sentinel identity or continuation cursor is used.
    fn first_keys(&mut self, kind: u32, excluded: Option<[u8; 32]>)
        -> ContentResult<Vec<[u8; 32]>>;
    /// Non-destructive sorted window of at most 64 complete keys strictly after
    /// `after`. None starts before every identity, including all-zero bytes.
    /// The caller seals membership before a pass; this is no snapshot across
    /// independently mutating jobs. Providers without this capability refuse
    /// explicitly rather than collect state or substitute destructive traversal.
    fn keys_after(&mut self, _kind: u32, _after: Option<[u8; 32]>) -> ContentResult<Vec<[u8; 32]>> {
        Err(crate::ContentError::ProviderFailure {
            what: "indexed construction key enumeration unavailable",
        })
    }
}
