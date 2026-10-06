//! The bounded units one acquisition is made of, and their failure classes.
use super::rows::{
    Abandoned, Begin, Directory, Entry, EntryKey, FileRoot, Job, NewEntry, Owner, Phase, Placed,
    Unplaced,
};
use super::work::{AcquisitionWork, Discarded};
use crate::port::PersistenceError;
use layerfs_content::ObjectId;
use std::{fmt, path::PathBuf};

/// Rows one read window returns at most.
pub const READ_WINDOW_ROWS: usize = 512;
/// Column bytes one read window returns at most, after its first row.
pub const READ_WINDOW_BYTES: usize = 256 * 1024;
/// Rows one write window carries at most.
pub const WRITE_WINDOW_ROWS: usize = 4096;
/// Payload bytes one write window carries at most.
pub const WRITE_WINDOW_BYTES: usize = 1024 * 1024;
/// Payload bytes one written row is charged beside its name and native path.
/// A caller sizing a window with this charge stays within the provider's.
pub const WRITE_ROW_BYTES: usize = 160;

/// Caller-selected read window, never above the port maxima.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Rows to return at most.
    pub rows: usize,
    /// Column bytes to return at most; the first row is always returned.
    pub bytes: usize,
}
impl Limits {
    /// The largest read window the port serves.
    pub const MAXIMUM: Self = Self {
        rows: READ_WINDOW_ROWS,
        bytes: READ_WINDOW_BYTES,
    };
    /// These limits held within the port maxima; a zero row count is one row.
    pub fn clamped(self) -> Self {
        Self {
            rows: self.rows.clamp(1, READ_WINDOW_ROWS),
            bytes: self.bytes.min(READ_WINDOW_BYTES),
        }
    }
}

/// One attempted unit's failure. Nothing here is retried by the provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcquisitionError {
    /// The provider's own typed outcome, including refusal and uncertainty.
    Persistence(PersistenceError),
    /// The owner is not a live operation of this provider session.
    Stale,
    /// A window exceeds the port's row or byte maximum, or a row is malformed.
    Bounds,
    /// A later path of one native identity carried different evidence, or a
    /// row the unit addressed was not in the state the unit requires. The
    /// unit changed nothing.
    Changed {
        /// Acquisition position of the row that did not match.
        position: u64,
    },
}
impl fmt::Display for AcquisitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "acquisition backing: {self:?}")
    }
}
impl std::error::Error for AcquisitionError {}
impl From<PersistenceError> for AcquisitionError {
    fn from(error: PersistenceError) -> Self {
        Self::Persistence(error)
    }
}
/// Result of one acquisition unit.
pub type AcquisitionResult<T> = Result<T, AcquisitionError>;

/// Provider-owned working state of initial acquisitions.
///
/// Every method is one attempt and one short provider transaction. A read
/// window is resumed from the last key it returned. Entry order is acquisition
/// order: breadth first, each directory's children by name bytes.
pub trait Acquisition: Send + Sync {
    /// Starts one operation and returns its owner.
    fn begin(&self, facts: &Begin) -> AcquisitionResult<Owner>;
    /// Native directory holding files this backing mutates, when it has one.
    /// A source containing it would acquire the backing itself.
    fn placement(&self) -> Option<PathBuf>;
    /// Appends one window of entries. Returns the canonical position of every
    /// positioned regular file, in window order.
    fn put_entries(&self, owner: Owner, entries: &[NewEntry]) -> AcquisitionResult<Vec<u64>>;
    /// Unpositioned children of one directory after `after`, in name order.
    fn unplaced_children(
        &self,
        owner: Owner,
        parent: u64,
        after: Option<&[u8]>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Unplaced>>;
    /// Gives one window of unpositioned children their positions. Returns the
    /// canonical position of every regular file, in window order.
    fn place_children(
        &self,
        owner: Owner,
        parent: u64,
        placed: &[Placed],
    ) -> AcquisitionResult<Vec<u64>>;
    /// Directories after one position, in position order.
    fn directories(
        &self,
        owner: Owner,
        after: Option<u64>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Directory>>;
    /// Native path of the directory at one position.
    fn directory_path(&self, owner: Owner, position: u64) -> AcquisitionResult<Option<Vec<u8>>>;
    /// First paths of native identities after one position, in position order.
    fn jobs(&self, owner: Owner, after: Option<u64>, limits: Limits)
        -> AcquisitionResult<Vec<Job>>;
    /// The first path of the identity whose canonical position is `position`.
    fn job(&self, owner: Owner, position: u64) -> AcquisitionResult<Option<Job>>;
    /// Records constructed roots by canonical position, each exactly once.
    fn complete_files(&self, owner: Owner, roots: &[(u64, ObjectId)]) -> AcquisitionResult<()>;
    /// Constructed roots and later-path counts after one position.
    fn file_roots(
        &self,
        owner: Owner,
        after: Option<u64>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<FileRoot>>;
    /// Entries after one key, in acquisition order.
    fn entries(
        &self,
        owner: Owner,
        after: Option<&EntryKey>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Entry>>;
    /// Records directory content roots by directory position, each exactly once.
    fn set_directory_roots(&self, owner: Owner, roots: &[(u64, ObjectId)])
        -> AcquisitionResult<()>;
    /// Records the operation's phase.
    fn advance(&self, owner: Owner, phase: Phase) -> AcquisitionResult<()>;
    /// Removes at most `rows` working rows of this operation.
    fn discard(&self, owner: Owner, rows: usize) -> AcquisitionResult<Discarded>;
    /// Removes the operation record. Refused while working rows remain.
    fn release(&self, owner: Owner) -> AcquisitionResult<()>;
    /// Exact logical charges of this operation.
    fn work(&self, owner: Owner) -> AcquisitionResult<AcquisitionWork>;
    /// Operations other provider sessions began, after one operation identity.
    fn abandoned(&self, after: Option<u64>, limit: usize) -> AcquisitionResult<Vec<Abandoned>>;
    /// Removes at most `rows` working rows of one abandoned operation, and its
    /// record once none remain. Calling this is the caller's statement that the
    /// operation's former owner is fenced; the provider cannot establish that.
    fn discard_abandoned(&self, abandoned: Owner, rows: usize) -> AcquisitionResult<Discarded>;
}
