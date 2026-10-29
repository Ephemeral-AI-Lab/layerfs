//! Bounded history records, explicit operation inputs and typed outcomes.
//!
//! Every value here is portable data: a record read from the catalog, a request
//! assembled by the caller, or an outcome the catalog returns. No type in this
//! module holds a connection, a handle, a path, a clock or a live Workspace, so
//! the same inputs produce the same operation from a direct test client, a
//! service thread or a future Workspace.

use crate::error::{HistoryError, HistoryResult};
use crate::identity::{BranchId, CommitId, HistoryName, LayerId, LayerStackId, NodeId};
use layerfs_content::ObjectId;

/// Default and maximum records one bounded page returns.
pub const MAXIMUM_PAGE_RECORDS: u16 = 128;
/// Largest encoded page result the service delivers for one history reply.
pub const MAXIMUM_RESULT_BYTES: usize = 16 * 1024;
/// Largest continuation value any history query accepts or returns.
pub const MAXIMUM_CURSOR_BYTES: usize = 160;

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
    /// Immutable source node selected when this Branch was created.
    pub forked_from: Option<NodeId>,
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
    /// The other side of a Branch merge: a Commit or a Layer.
    pub second_parent: Option<NodeId>,
    /// Who made the Commit and how.
    pub meta: CommitMeta,
}

/// How a Commit was made.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitVia {
    /// A Workspace Commit (a mounted Workspace's captured changes).
    Workspace,
    /// A direct Branch file operation.
    Files,
    /// A Branch merge.
    Merge,
}

impl CommitVia {
    /// Stored code.
    pub const fn code(self) -> u8 {
        match self {
            Self::Workspace => 1,
            Self::Files => 2,
            Self::Merge => 3,
        }
    }

    /// Decodes a stored code.
    pub fn from_code(code: u8) -> crate::error::HistoryResult<Self> {
        match code {
            1 => Ok(Self::Workspace),
            2 => Ok(Self::Files),
            3 => Ok(Self::Merge),
            _ => Err(crate::error::HistoryError::Integrity("Commit via")),
        }
    }
}

/// Maximum encoded UTF-8 bytes in a Commit or execution message.
pub const MAXIMUM_MESSAGE_BYTES: usize = 4096;

/// Largest encoded UTF-8 bytes in a Commit author or one label.
pub const MAXIMUM_ATTRIBUTION_BYTES: usize = 4096;
/// Largest number of labels on one Commit.
pub const MAXIMUM_LABELS: usize = 64;
/// Largest encoded UTF-8 bytes of the complete labels JSON array.
pub const MAXIMUM_LABELS_JSON_BYTES: usize = 4096;

/// Commit provenance and publication facts. Branch, via, message, merge origin
/// and displaced head participate in identity; author, labels, time, byte
/// statistics, LWW details and merge units are creation-time annotations or
/// persisted facts outside that identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitMeta {
    /// Branch the Commit was made on; the catalog sets it from the request.
    pub branch: Option<BranchId>,
    /// How it was made.
    pub via: CommitVia,
    /// Description of the filesystem change; participates in Commit identity.
    pub message: Option<String>,
    /// Person or agent that made the Commit; outside Commit identity.
    pub author: Option<String>,
    /// Free-form creation-time labels, preserving order and duplicates.
    pub labels: Vec<String>,
    /// Head a merge-on-commit started from, when one ran.
    pub merged_from: Option<NodeId>,
    /// Persisted newer-write-wins decisions.
    pub lww: Vec<layerfs_content::merge::LwwDecision>,
    /// True when a transport observation omitted persisted decisions.
    pub lww_truncated: bool,
    /// The merge units a Branch merge ran with (#352 F-057), persisted
    /// outside identity like the LWW decisions.
    pub units: Vec<layerfs_content::merge::MergeUnit>,
    /// Snapshot logical size and this producer's acknowledged new bytes.
    pub statistics: CommitStatistics,
    /// Head displaced by the original forced publication.
    pub displaced: Option<CommitId>,
    /// Milliseconds since the Unix epoch; the catalog sets it on insert.
    pub time_ms: i64,
}

impl CommitMeta {
    /// Metadata of one way of committing, with no attribution.
    pub const fn new(via: CommitVia) -> Self {
        Self {
            branch: None,
            via,
            message: None,
            author: None,
            labels: Vec::new(),
            merged_from: None,
            lww: Vec::new(),
            lww_truncated: false,
            units: Vec::new(),
            statistics: CommitStatistics::ZERO,
            displaced: None,
            time_ms: 0,
        }
    }

    /// Checks message, author and label bounds before storage or transport.
    pub fn check(&self) -> crate::error::HistoryResult<()> {
        fn text(field: &str, what: &'static str) -> crate::error::HistoryResult<()> {
            if field.len() > MAXIMUM_ATTRIBUTION_BYTES
                || field
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\t')
            {
                return Err(crate::error::HistoryError::InvalidInput(what));
            }
            Ok(())
        }
        if let Some(message) = &self.message {
            text(message, "Commit message")?;
        }
        if let Some(author) = &self.author {
            text(author, "Commit author")?;
        }
        if self.labels.len() > MAXIMUM_LABELS {
            return Err(crate::error::HistoryError::InvalidInput("Commit labels"));
        }
        for label in &self.labels {
            text(label, "Commit label")?;
        }
        if crate::labels::encode(&self.labels).len() > MAXIMUM_LABELS_JSON_BYTES {
            return Err(crate::error::HistoryError::InvalidInput("Commit labels JSON"));
        }
        Ok(())
    }
}

/// One immutable Layer publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerRecord {
    /// How many earlier main Layers this publication displaced, recorded
    /// atomically. They are the newest `overwrote_count` Layers below it in
    /// the stack chain; `HistoryCatalog::overwritten` pages them.
    pub overwrote_count: u64,
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
    /// Complete logical regular-file bytes, counting every hardlink path.
    pub logical: u64,
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
    /// Complete logical regular-file bytes of the acquired root.
    pub logical: u64,
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

/// One Commit publication against an expected Branch head, in one transaction.
///
/// Without `force` the Branch must still hold `expected_head` and
/// `expected_base`; otherwise the request is refused with `HeadMoved` and
/// nothing changes. With `force` the candidate overwrites whatever head the
/// Branch holds, keeps `expected_head` as its parent and reports the head it
/// displaced. There is no stage row and no token: the expected head is the
/// only precondition, carried by the request itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitRequest {
    /// Branch the candidate was constructed for.
    pub branch: BranchId,
    /// Head the Branch must hold; also the new Commit's parent.
    pub expected_head: Option<CommitId>,
    /// Base Layer the Branch must hold; also the new Commit's base.
    pub expected_base: LayerId,
    /// Effective root of `expected_head` (or of the base Layer without one).
    pub expected_root: ObjectId,
    /// Saved root of the candidate.
    pub candidate_root: ObjectId,
    /// Filesystem profile the candidate was constructed under.
    pub profile: ObjectId,
    /// Allocation scope the candidate was constructed under.
    pub scope: ObjectId,
    /// The other side of a Branch merge, recorded as the second parent.
    pub second_parent: Option<NodeId>,
    /// How the Commit was made; the catalog fills its Branch and time.
    pub meta: CommitMeta,
    /// Overwrite a head that moved instead of refusing.
    pub force: bool,
}

/// One side of a Branch merge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MergeSide {
    /// The stack's main line of Layers.
    Main,
    /// A Branch.
    Branch(BranchId),
}

/// The first state of a Branch created by a merge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MergeHead {
    /// Fast-forward: the Branch starts at an existing node, a Commit (with
    /// that Commit's base) or a Layer (as its base, with no head).
    Node(NodeId),
    /// One new Commit with two parents.
    Commit(Box<MergeCommit>),
}

/// The Commit a non-fast-forward Branch merge creates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeCommit {
    /// Base Layer of the new Branch and Commit.
    pub base: LayerId,
    /// First parent, when the left side is a Commit.
    pub parent: Option<CommitId>,
    /// The right side.
    pub second_parent: NodeId,
    /// Saved merged root.
    pub root: ObjectId,
    /// How the Commit was made; the catalog fills its Branch and time.
    pub meta: CommitMeta,
}

/// Creates one Branch from two sides that must still be at the nodes the
/// caller merged, in one transaction. Neither side changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchMergeRequest {
    /// Stack of both sides and the new Branch.
    pub stack: LayerStackId,
    /// New Branch identity supplied by the application authority.
    pub branch: BranchId,
    /// New Branch name.
    pub name: HistoryName,
    /// Both sides and the node each must still be at.
    pub sides: [(MergeSide, NodeId); 2],
    /// The new Branch's first state.
    pub head: MergeHead,
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

/// Successful result of one Commit publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitOutcome {
    /// A new immutable Commit was inserted and the Branch advanced.
    Committed {
        /// The Commit the Branch now holds.
        record: Box<CommitRecord>,
        /// Head a forced publication overwrote, when it was not the parent.
        displaced: Option<CommitId>,
    },
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
    /// A new immutable Layer was inserted on the stack head and the stack
    /// advanced. Publication is overwrite-only: the Layer's
    /// `overwrote_count` counts the stack's Layers since the Branch's base
    /// that the published Commit does not contain; they stay in history and
    /// `HistoryCatalog::overwritten` lists them.
    Added {
        /// The new Layer.
        layer: LayerRecord,
    },
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
            _ => return Err(HistoryError::InvalidInput("inode kind")),
        })
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
encoded_width!(BranchRecord, 200);
encoded_width!(CommitRecord, 12608);
encoded_width!(LayerRecord, 176);

/// Byte facts acknowledged by a construction producer and stored at publication.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CommitStatistics {
    /// Complete snapshot regular-file bytes, counting every namespace link.
    pub logical: u64,
    /// Newly inserted canonical object bytes from this Save.
    pub new_canonical: u64,
    /// Complete new pack bytes from this Save, including any losing-race packs.
    pub new_stored: u64,
    /// Complete newly published metadata-domain pack bytes from this Save.
    pub new_metadata: u64,
}
impl CommitStatistics {
    /// No acknowledged bytes, for callers that have no construction output.
    pub const ZERO: Self = Self {
        logical: 0,
        new_canonical: 0,
        new_stored: 0,
        new_metadata: 0,
    };
}

/// Store-wide byte facts read from publication-maintained totals.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoreStatistics {
    /// Sum of logical bytes across immutable Commit and Layer history records.
    pub logical: u64,
    /// Unique registered canonical object bytes, including structural objects.
    pub canonical: u64,
    /// Published physical pack bytes, including structural objects.
    pub stored: u64,
    /// Metadata-domain pack bytes plus occupied SQLite space outside all pack bodies.
    pub metadata: u64,
    /// SQLite logical pages in bytes, including free pages and pages currently in WAL.
    /// This is database size, not main-file length or extra WAL-file allocation.
    pub file: u64,
    /// Free SQLite pages in bytes, reusable within the file.
    pub free: u64,
    /// What Init imported and what it cost; absent until Init records it.
    pub init: Option<InitStatistics>,
}

/// Contents one `file_roots` demand asks about.
pub const FILE_DIGEST_WINDOW: usize = 64;

/// The whole-content digest of one saved regular-file root (#364): BLAKE3
/// of its `length` bytes, recorded by the Commit that built the root from
/// exactly those bytes. A file with the same digest and length has the same
/// bytes, so that root serves it without reading or building it again.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileDigest {
    /// The file root's object identity bytes.
    pub root: [u8; 32],
    /// The file's logical length.
    pub length: u64,
    /// BLAKE3 of the file's bytes.
    pub digest: [u8; 32],
}

/// One name binding a saved tree holds: `name` in directory `parent` names
/// inode `child`. A recorded binding is a hint, not a fact of any one tree:
/// a reader checks it against the tree it asks about before using it.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BindingHint {
    /// The bound inode's serial.
    pub child: u64,
    /// The binding directory's serial.
    pub parent: u64,
    /// The bound name's bytes.
    pub name: Vec<u8>,
}

/// One Init's import counts and the Store totals it left, recorded once at seal.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InitStatistics {
    /// Source entries scanned, including the root directory.
    pub entries: u64,
    /// Regular-file identities constructed; extra links of one identity are not counted.
    pub files: u64,
    /// Bytes of those identities.
    pub bytes: u64,
    /// Store canonical bytes when Init finished.
    pub canonical: u64,
    /// Store pack bytes when Init finished.
    pub stored: u64,
    /// Store metadata bytes, as [`StoreStatistics::metadata`], when Init finished.
    pub metadata: u64,
    /// Init's wall time in milliseconds, up to the first Branch.
    pub millis: u64,
}
