//! Bounded history records, explicit operation inputs and typed outcomes.
//!
//! Every value here is portable data: a record read from the catalog, a request
//! assembled by the caller, or an outcome the catalog returns. No type in this
//! module holds a connection, a handle, a path, a clock or a live Workspace, so
//! the same inputs produce the same operation from a direct test client, a
//! service thread or a future Workspace.

use crate::error::{HistoryError, HistoryResult};
use crate::identity::{
    BranchId, CommitId, HistoryName, LayerId, LayerStackId, StageToken, WorkspaceId,
};
use layerfs_content::ObjectId;

/// Default and maximum records one bounded page returns.
pub const MAXIMUM_PAGE_RECORDS: u16 = 128;
/// Largest encoded page result the service delivers for one history reply.
pub const MAXIMUM_RESULT_BYTES: usize = 16 * 1024;
/// Largest continuation value any history query accepts or returns.
pub const MAXIMUM_CURSOR_BYTES: usize = 160;
/// Rows one lineage walk may examine before it reports exhaustion.
///
/// This is a work ceiling independent of every C1 traversal budget. Reaching it
/// is `Capacity`, never an empty result: an over-budget walk is unproven, and
/// reporting absence for it would be a false negative.
pub const MAXIMUM_LINEAGE_ROWS: u64 = 4096;

/// One LayerStack: a named linear Layer publication timeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerStackRecord {
    /// Stack identity.
    pub id: LayerStackId,
    /// Authority-local unique name.
    pub name: HistoryName,
    /// Allocation scope shared by every Branch and Commit in this stack.
    pub scope: ObjectId,
    /// Frozen filesystem profile identity of this stack.
    pub profile: ObjectId,
    /// Current conditional head Layer.
    pub head_layer: LayerId,
}

/// One Branch: a base Layer plus an optional head Commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchRecord {
    /// Branch identity.
    pub id: BranchId,
    /// Owning stack.
    pub stack: LayerStackId,
    /// Authority-local unique name inside the stack.
    pub name: HistoryName,
    /// Base Layer of this Branch.
    pub base_layer: LayerId,
    /// Recorded head Commit, absent for a freshly forked Branch.
    pub head_commit: Option<CommitId>,
}

/// One coherent Branch snapshot with its resolved roots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchSnapshot {
    /// The Branch metadata row.
    pub branch: BranchRecord,
    /// Root of the head Commit, absent when the Branch has no Commit.
    pub head_root: Option<ObjectId>,
    /// Root of the base Layer.
    pub base_root: ObjectId,
    /// Head Commit root when present, otherwise the base Layer root.
    pub effective_root: ObjectId,
    /// Allocation scope inherited from the stack.
    pub scope: ObjectId,
    /// Frozen filesystem profile identity of the stack.
    pub profile: ObjectId,
}

/// One immutable Commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitRecord {
    /// Commit identity.
    pub id: CommitId,
    /// Owning stack.
    pub stack: LayerStackId,
    /// Complete filesystem root this Commit records.
    pub root: ObjectId,
    /// Recorded ancestry; not a diff and not a conflict resolution.
    pub parent: Option<CommitId>,
    /// Base Layer the Commit was built from.
    pub base_layer: LayerId,
}

/// One immutable Layer publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerRecord {
    /// Layer identity.
    pub id: LayerId,
    /// Owning stack.
    pub stack: LayerStackId,
    /// Previous publication in this stack; absent only for genesis.
    pub parent: Option<LayerId>,
    /// Complete filesystem root this Layer publishes.
    pub root: ObjectId,
    /// Branch that published it; absent only for genesis.
    pub source_branch: Option<BranchId>,
    /// Commit whose root was published; absent only for genesis.
    pub source_commit: Option<CommitId>,
}

/// One frozen Workspace stage: a saved candidate plus immutable expectations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StageRecord {
    /// Producer incarnation that owns this stage.
    pub workspace: WorkspaceId,
    /// Exact token drawn from the catalog counter when the stage was inserted.
    pub token: StageToken,
    /// Stack the stage was captured against.
    pub stack: LayerStackId,
    /// Branch the stage was captured against.
    pub branch: BranchId,
    /// Head Commit the Branch held when the stage was captured.
    pub expected_head: Option<CommitId>,
    /// Base Layer the Branch held when the stage was captured.
    pub expected_base: LayerId,
    /// Effective root the Branch held when the stage was captured.
    pub expected_root: ObjectId,
    /// Root the candidate was constructed from.
    pub construction_base_root: ObjectId,
    /// Base Layer the Commit would record.
    pub intended_commit_base: LayerId,
    /// Saved root of the staged candidate.
    pub candidate_root: ObjectId,
    /// Filesystem profile of the captured Branch.
    pub profile: ObjectId,
    /// Allocation scope of the captured Branch.
    pub scope: ObjectId,
    /// Producer generation this candidate was built from.
    pub generation: u64,
}

/// One consumed half-open inode reservation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reservation {
    /// Scope the serials belong to.
    pub scope: ObjectId,
    /// First serial of the reservation.
    pub start: u64,
    /// Serials reserved; the range is `[start, start + count)`.
    pub count: u64,
}

impl Reservation {
    /// The exclusive end of the reserved range.
    ///
    /// The profile's serials end at `i64::MAX`, so an exclusive end above that
    /// value is not a range this profile can represent and is refused rather
    /// than reported as a number the caller could act on.
    pub fn end(&self) -> HistoryResult<u64> {
        self.start
            .checked_add(self.count)
            .filter(|end| *end <= i64::MAX as u64)
            .ok_or(HistoryError::Capacity("inode reservation"))
    }
}

/// One bounded list request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page {
    /// Continuation returned by a previous page; absent for the first page.
    pub cursor: Option<Vec<u8>>,
    /// Records requested; one to 128.
    pub limit: u16,
}

impl Page {
    /// Checks the declared page bounds before any row is examined.
    pub fn check(&self) -> HistoryResult<()> {
        if self.limit == 0 || self.limit > MAXIMUM_PAGE_RECORDS {
            return Err(HistoryError::InvalidInput("page limit"));
        }
        if self
            .cursor
            .as_ref()
            .is_some_and(|cursor| cursor.len() > MAXIMUM_CURSOR_BYTES)
        {
            return Err(HistoryError::InvalidInput("page cursor"));
        }
        Ok(())
    }
}

/// One bounded page of records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageResult<T> {
    /// Records in query order.
    pub records: Vec<T>,
    /// Continuation when more records remain; absent at the end of the range.
    pub continuation: Option<Vec<u8>>,
}

/// Stack creation request: the genesis publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StackInitialization {
    /// Stack identity supplied by the application authority.
    pub stack: LayerStackId,
    /// Authority-local unique stack name.
    pub name: HistoryName,
    /// Allocation scope every Branch in this stack will share.
    pub scope: ObjectId,
    /// Frozen filesystem profile identity.
    pub profile: ObjectId,
    /// Root of the already-published genesis filesystem.
    pub genesis_root: ObjectId,
}

/// Where a new Branch starts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForkSource {
    /// Fork directly from a published Layer.
    Layer(LayerId),
    /// Fork from a Commit reached through the named Branch.
    Commit {
        /// Branch whose history is being read.
        branch: BranchId,
        /// Selected ancestor Commit.
        commit: CommitId,
    },
}

/// Branch creation request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForkRequest {
    /// Stack the new Branch belongs to.
    pub stack: LayerStackId,
    /// New Branch identity supplied by the application authority.
    pub branch: BranchId,
    /// Authority-local unique Branch name inside the stack.
    pub name: HistoryName,
    /// Selected starting point.
    pub source: ForkSource,
}

/// Stage insertion request: one saved candidate with its frozen expectations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StageRequest {
    /// Producer incarnation that will own the stage.
    pub workspace: WorkspaceId,
    /// Branch the candidate was constructed against.
    pub branch: BranchId,
    /// Head Commit captured before construction.
    pub expected_head: Option<CommitId>,
    /// Base Layer captured before construction.
    pub expected_base: LayerId,
    /// Effective Branch root captured before construction.
    pub expected_root: ObjectId,
    /// Root C1 actually constructed from.
    pub construction_base_root: ObjectId,
    /// Base Layer the Commit would record.
    pub intended_commit_base: LayerId,
    /// Saved root of the candidate.
    pub candidate_root: ObjectId,
    /// Filesystem profile the candidate was constructed under.
    pub profile: ObjectId,
    /// Allocation scope the candidate was constructed under.
    pub scope: ObjectId,
    /// Producer generation this candidate was built from.
    pub generation: u64,
}

/// Commit request naming one exact stage token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitStagedRequest {
    /// Producer incarnation that owns the stage.
    pub workspace: WorkspaceId,
    /// Exact token the caller believes it holds.
    pub token: StageToken,
}

/// Stage removal request naming one exact stage token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscardRequest {
    /// Producer incarnation that owns the stage.
    pub workspace: WorkspaceId,
    /// Exact token the caller believes it holds.
    pub token: StageToken,
}

/// Layer publication request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddLayerRequest {
    /// Stack that receives the Layer.
    pub stack: LayerStackId,
    /// Branch that published the Commit.
    pub branch: BranchId,
    /// Selected source Commit.
    pub commit: CommitId,
    /// Stack head the caller expects.
    pub expected_stack_head: LayerId,
    /// Branch base the caller expects.
    pub expected_branch_base: LayerId,
}

/// Inode reservation request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReserveRequest {
    /// Allocation scope that owns the serials.
    pub scope: ObjectId,
    /// Serials requested; the reservation is consumed even if unused.
    pub count: u64,
}

/// Ancestry read of one Branch's Commit history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitHistoryRequest {
    /// Branch whose ancestry is walked.
    pub branch: BranchId,
    /// Immutable starting Commit; absent means the live Branch head.
    pub start: Option<CommitId>,
    /// Continuation returned by a previous page.
    pub cursor: Option<Vec<u8>>,
    /// Records requested; one to 128.
    pub limit: u16,
}

/// Publication read of one stack's Layer history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerHistoryRequest {
    /// Stack whose publications are walked.
    pub stack: LayerStackId,
    /// Immutable starting Layer; absent means the live stack head.
    pub start: Option<LayerId>,
    /// Continuation returned by a previous page.
    pub cursor: Option<Vec<u8>>,
    /// Records requested; one to 128.
    pub limit: u16,
}

/// Successful result of committing one exact stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitStagedOutcome {
    /// A new immutable Commit was inserted and the Branch advanced.
    Committed(CommitRecord),
    /// The candidate root and intended base already describe the Branch.
    UpToDate {
        /// Head Commit the Branch holds, absent when it has none yet.
        head: Option<CommitId>,
        /// Effective root the Branch already has.
        root: ObjectId,
    },
}

/// Successful result of a Layer publication attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AddLayerOutcome {
    /// A new immutable Layer was inserted and the stack advanced.
    Added(LayerRecord),
    /// The selected source Commit already published in this stack.
    UpToDate {
        /// Layer that already publishes this source.
        layer: LayerId,
    },
    /// The Commit root equals its base Layer root, so publication is a no-op.
    NoChanges {
        /// Stack head, unchanged.
        head: LayerId,
    },
}

/// Successful result of an exact stage removal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscardOutcome {
    /// The exact stage existed and was removed.
    Removed,
    /// No stage exists for that Workspace incarnation.
    Absent,
}

/// Kind of one manifest entry.
///
/// The codes are C1's persisted inode-kind codes exactly, so a manifest kind and
/// a prepared-update inode kind are the same number and cannot be confused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RecordKind {
    /// A regular file backed by an already published file root.
    RegularFile = 1,
    /// A directory.
    Directory = 2,
    /// A symbolic link carrying an inline bounded target.
    Symlink = 3,
}

impl RecordKind {
    /// The C1 inode kind code of this kind.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Checks and wraps one C1 inode kind code.
    pub fn from_code(code: u8) -> HistoryResult<Self> {
        Ok(match code {
            1 => Self::RegularFile,
            2 => Self::Directory,
            3 => Self::Symlink,
            _ => return Err(HistoryError::InvalidInput("manifest kind")),
        })
    }
}

/// One pathless entry of a bounded namespace manifest.
///
/// The entry names its parent by index rather than by path, so a manifest is a
/// tree by construction and needs no resolution, no host scan and no second
/// import algorithm. Entry zero is always the root directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestEntry {
    /// Index of the parent entry; zero for the root entry itself.
    pub parent: u16,
    /// Canonical component name; empty for the root entry.
    pub name: Vec<u8>,
    /// Entry kind.
    pub kind: RecordKind,
    /// Portable permission bits.
    pub mode: u32,
    /// Portable modification time, seconds since the Unix epoch.
    pub mtime_seconds: i64,
    /// Portable modification time, fractional second.
    pub mtime_nanoseconds: u32,
    /// Already published file root; regular files only.
    pub content: Option<ObjectId>,
    /// Inline symbolic-link target; symlinks only.
    pub target: Option<Vec<u8>>,
}

/// One bounded logical namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamespaceManifest {
    /// Entries in declaration order; the root is entry zero.
    pub entries: Vec<ManifestEntry>,
}

impl NamespaceManifest {
    /// The single-directory case: one root directory and nothing else.
    pub fn empty(entry: ManifestEntry) -> Self {
        Self {
            entries: vec![entry],
        }
    }

    /// Checks every entry and every parent relation before any object is built.
    ///
    /// The root is entry zero and must be a directory with no name, no content
    /// and no target. Every other entry names a strictly earlier parent, so the
    /// manifest is acyclic and every entry is reachable from the root.
    pub fn check(&self) -> HistoryResult<()> {
        if self.entries.is_empty() {
            return Err(HistoryError::InvalidInput("manifest size"));
        }
        let root = &self.entries[0];
        if root.kind != RecordKind::Directory
            || !root.name.is_empty()
            || root.parent != 0
            || root.content.is_some()
            || root.target.is_some()
        {
            return Err(HistoryError::InvalidInput("manifest root"));
        }
        for (index, entry) in self.entries.iter().enumerate() {
            if index != 0 {
                if entry.name.is_empty() || entry.name.len() > 255 || entry.name.contains(&0) {
                    return Err(HistoryError::InvalidInput("manifest name"));
                }
                if usize::from(entry.parent) >= index {
                    return Err(HistoryError::InvalidInput("manifest parent"));
                }
            }
            if entry.mtime_nanoseconds > 999_999_999 {
                return Err(HistoryError::InvalidInput("manifest mtime"));
            }
            let mask = match entry.kind {
                RecordKind::Directory => 0o1777,
                RecordKind::RegularFile | RecordKind::Symlink => 0o777,
            };
            if entry.mode & !mask != 0 {
                return Err(HistoryError::InvalidInput("manifest mode"));
            }
            if entry.kind == RecordKind::Symlink && entry.mode != 0o777 {
                return Err(HistoryError::InvalidInput("manifest symlink mode"));
            }
            match entry.kind {
                RecordKind::RegularFile => {
                    if entry.content.is_none() || entry.target.is_some() {
                        return Err(HistoryError::InvalidInput("manifest file root"));
                    }
                }
                RecordKind::Symlink => {
                    let target = entry
                        .target
                        .as_ref()
                        .ok_or(HistoryError::InvalidInput("manifest symlink target"))?;
                    if target.len() > 4096 || target.contains(&0) || entry.content.is_some() {
                        return Err(HistoryError::InvalidInput("manifest symlink target"));
                    }
                }
                RecordKind::Directory => {
                    if entry.content.is_some() || entry.target.is_some() {
                        return Err(HistoryError::InvalidInput("manifest directory"));
                    }
                }
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for entry in self.entries.iter().skip(1) {
            if !seen.insert((entry.parent, entry.name.as_slice())) {
                return Err(HistoryError::InvalidInput("manifest duplicate name"));
            }
        }
        Ok(())
    }
}

/// Worst-case encoded bytes of one record in a wire page.
///
/// The service and the bridge freeze these figures together: a page holds at
/// most [`MAXIMUM_PAGE_RECORDS`] records and at most [`MAXIMUM_RESULT_BYTES`]
/// encoded bytes, and a page that cannot carry the caller's full `limit` inside
/// the byte budget is cut short with a continuation. The catalog therefore has
/// to know the same widths the codec writes.
///
/// Each figure is the exact sum of the record's fields at their declared
/// maxima, including every length prefix, and an understated figure is a real
/// defect: the catalog would pack a page the codec then refuses. The service's
/// external test measures the encoded width of a maximal record of each kind
/// and holds it to the figure here.
macro_rules! encoded_width {
    ($name:ident, $bytes:expr) => {
        impl $name {
            /// Largest encoded bytes of this record.
            pub const MAXIMUM_ENCODED_BYTES: usize = $bytes;
        }
    };
}

encoded_width!(LayerStackRecord, 179);
encoded_width!(BranchRecord, 166);
encoded_width!(CommitRecord, 149);
encoded_width!(LayerRecord, 168);
encoded_width!(StageRecord, 342);
