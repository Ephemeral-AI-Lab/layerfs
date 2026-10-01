//! Closed draft ownership and final publication, supplied by one operation owner.
use super::draft_record::DraftRecord;
use crate::filesystem::state::{StateScope, StateSelection, StateTable};
use crate::{ContentError, ContentResult, ObjectId};

/// Supported aggregate metadata shape, independently of payload bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftCapacity {
    records: u64,
    encoded_bytes: u64,
    scratch_bytes: u64,
}
impl Default for DraftCapacity {
    fn default() -> Self {
        Self::new(16 * 1024 * 1024).expect("default draft scratch format")
    }
}
impl DraftCapacity {
    /// Capture configured nativeS without growing metadata/RAM populations.
    /// Formal representability does not qualify the provider or physical domain.
    pub fn new(scratch_bytes: u64) -> ContentResult<Self> {
        if !(16 * 1024 * 1024..=(1_u64 << 40) - 4096).contains(&scratch_bytes)
            || scratch_bytes % 4096 != 0
            || usize::try_from(scratch_bytes).is_err()
            || i64::try_from(scratch_bytes).is_err()
            || scratch_bytes / 4096 > u64::from(u32::MAX)
        {
            return Err(ContentError::InvalidOrderingRecord("draft scratch budget"));
        }
        Ok(Self {
            records: 65_536,
            encoded_bytes: super::EDIT_DEFERRED_LIMIT as u64,
            scratch_bytes,
        })
    }
    /// Validate the fixed metadata class and captured formally representableS.
    pub fn validated(self) -> ContentResult<Self> {
        if Self::new(self.scratch_bytes)? != self {
            return Err(ContentError::UnsupportedPolicy {
                field: "draft capacity",
            });
        }
        Ok(self)
    }
    /// Aggregate live record count ceiling.
    pub const fn records(self) -> u64 {
        self.records
    }
    /// Aggregate live framed metadata ceiling.
    pub const fn encoded_bytes(self) -> u64 {
        self.encoded_bytes
    }
    /// Native reserved class, not a resident-memory claim.
    pub const fn scratch_bytes(self) -> u64 {
        self.scratch_bytes
    }
}
/// Exact live bound profile6 draft authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftScope {
    state: StateScope,
    capacity: DraftCapacity,
}
impl DraftScope {
    /// Select the dedicated draft phase/table of an already bound issued owner.
    pub fn new(selection: StateSelection, capacity: DraftCapacity) -> ContentResult<Self> {
        Ok(Self {
            state: StateScope::new(selection, 6, StateTable::DraftHeaders)?,
            capacity: capacity.validated()?,
        })
    }
    /// Exact live operation association.
    pub fn state(&self) -> &StateScope {
        &self.state
    }
    /// Captured class, before producer effects.
    pub const fn capacity(&self) -> DraftCapacity {
        self.capacity
    }
    /// Full selector/token/owner/phase/table plus capacity24.
    pub fn as_bytes(&self) -> [u8; 105] {
        let mut bytes = [0; 105];
        bytes[..81].copy_from_slice(&self.state.as_bytes());
        bytes[81..89].copy_from_slice(&self.capacity.records.to_be_bytes());
        bytes[89..97].copy_from_slice(&self.capacity.encoded_bytes.to_be_bytes());
        bytes[97..].copy_from_slice(&self.capacity.scratch_bytes.to_be_bytes());
        bytes
    }
}
/// Advancing detached work with exact live count, never an inferred ownership fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftJob {
    /// Issued positive monotone queue sequence.
    pub sequence: u64,
    /// Exact selected draft identity/key.
    pub id: ObjectId,
    /// Current checked parent plus selected-root count.
    pub links: u64,
}
/// Current and peak owned metadata, with work counts independent of timing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DraftStats {
    /// Current admitted framed metadata including associated facts.
    pub bytes: usize,
    /// Peak admitted metadata.
    pub peak_bytes: usize,
    /// Acknowledged unfinished bodies under the historical logical charge convention.
    pub deferred_body_bytes: usize,
    /// Peak prospective unfinished bodies; excludes associated metadata and spare capacity.
    pub peak_deferred_body_bytes: usize,
    /// Ready draft creations.
    pub created: u64,
    /// Retired draft bodies.
    pub retired: u64,
    /// Outgoing links retired.
    pub links_retired: u64,
    /// Detached jobs consumed, including revived jobs.
    pub jobs_consumed: u64,
    /// Native reference-insert prepare calls issued, including failed or uncertain attempts.
    pub reference_insert_prepare_calls: u64,
    /// Native reference-delete prepare calls issued, including failed or uncertain attempts.
    pub reference_delete_prepare_calls: u64,
    /// Native reference-insert row acknowledgements, including later rollback or COMMIT Unknown.
    pub reference_insert_rows: u64,
    /// Native reference-delete row acknowledgements, including later rollback or COMMIT Unknown.
    pub reference_delete_rows: u64,
}
/// One exclusive closed mutable lifecycle; no generic key/value mutation.
pub trait DraftState {
    /// Pure captured supported capacity, before canonical effects.
    fn capacity(&self) -> ContentResult<DraftCapacity>;
    /// Complete one Creating->Ready draft in bounded acknowledged batches.
    fn hold(&mut self, id: ObjectId, record: DraftRecord) -> ContentResult<()>;
    /// One bounded Ready body; a creating/retiring body cannot be adopted.
    fn get(&mut self, id: ObjectId) -> ContentResult<Option<DraftRecord>>;
    /// Acquire the new selected-root link before releasing the exact prior root.
    fn select_root(&mut self, before: Option<ObjectId>, after: ObjectId) -> ContentResult<()>;
    /// Acquire bounded temporary-summary pins, combining exact duplicate multiplicities.
    fn retain_temporaries(&mut self, ids: &[ObjectId]) -> ContentResult<()>;
    /// Release consumed temporary pins after replacement parent/root ownership exists.
    fn release_temporaries(&mut self, ids: &[ObjectId]) -> ContentResult<()>;
    /// Consume one temporary name, clear its matching selected root, and retire only it if zero.
    fn supersede(&mut self, id: ObjectId, expected_selected: Option<ObjectId>)
        -> ContentResult<()>;
    /// Exact FIRST outstanding sequence under the selected owner.
    fn next_job(&mut self) -> ContentResult<Option<DraftJob>>;
    /// Consume a revived job or retire a zero draft and its bounded outgoing edges.
    fn retire_job(&mut self, job: DraftJob) -> ContentResult<()>;
    /// Exact final draft resolution; stored identities return absent.
    fn resolved(&mut self, id: ObjectId) -> ContentResult<Option<ObjectId>>;
    /// Reserve Pending once. False means the exact canonical identity is Accepted.
    fn begin_emission(&mut self, draft: ObjectId, canonical: ObjectId) -> ContentResult<bool>;
    /// Known accepted output plus exact draft resolution; cannot acknowledge twice.
    fn accepted(&mut self, draft: ObjectId, canonical: ObjectId) -> ContentResult<()>;
    /// Known final logical retirement before file-state output, with bounded native steps.
    fn finish(&mut self) -> ContentResult<()>;
    /// Pure metadata-only failure terminalization; no SQL/refund/retry.
    fn abandon(&mut self);
    /// Current acknowledged owner/work facts.
    fn stats(&self) -> DraftStats;
}
