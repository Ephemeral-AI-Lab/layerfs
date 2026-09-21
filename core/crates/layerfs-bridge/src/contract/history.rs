//! History payloads: the closed query/command surface and its typed results.
//!
//! History uses two grouped opcodes and its own operation profile, so the legacy
//! opcode numbering, permission bits and payload grammar are untouched. The
//! bridge carries identities as the exact fixed-width byte strings C5 encodes;
//! it decodes no history record and depends on no history implementation.
//!
//! Nothing here is inferred. A caller states a complete request, an unknown
//! suboperation is refused before any mutation, and a reply is matched against
//! the request that produced it rather than being interpreted on its own.

use super::{Code, Failure, Root};

/// Operation profile of every history request and reply.
pub const HISTORY_PROFILE: u16 = 2;
/// Opcode of a read-only history query.
pub const QUERY_OPCODE: u8 = 6;
/// Opcode of a mutating history command.
pub const COMMAND_OPCODE: u8 = 7;
/// Encoded width of a Branch identity.
pub const BRANCH_BYTES: usize = 17;
/// Encoded width of a Commit identity.
pub const COMMIT_BYTES: usize = 33;
/// Encoded width of a LayerStack identity.
pub const STACK_BYTES: usize = 17;
/// Encoded width of a Layer identity.
pub const LAYER_BYTES: usize = 33;
/// Encoded width of a Workspace incarnation.
pub const WORKSPACE_BYTES: usize = 32;
/// Largest bytes of one authority-local history name.
pub const NAME_MAX_BYTES: usize = 63;
/// Largest continuation value a history page accepts or returns.
pub const CURSOR_BYTES: usize = 160;
/// Largest records one history page returns.
pub const PAGE_RECORDS: u16 = 128;
/// Complete history terminal result budget, including tags and prefixes.
pub const HISTORY_RESULT_BYTES: usize = 16 * 1024;
/// Widest profile-2 failure: prefix/version, Branch conflict and full retained stage.
pub const HISTORY_FAILURE_BYTES: usize = 482;
/// Largest entries one init manifest declares, including its root.
pub const MANIFEST_ENTRIES: usize = 128;
/// Largest bytes of one symlink target inside a manifest.
pub const MANIFEST_TARGET_BYTES: usize = 4096;

/// The permission bit one opcode requires.
///
/// The mapping is total and explicit rather than a shift of an unchecked
/// opcode, so unknown and daemon-control opcodes have no Store permission bit.
/// A legacy grant mask of 31 therefore grants neither history opcode.
pub const fn permission_bit(opcode: u8) -> Option<u8> {
    match opcode {
        1 => Some(1 << 0),
        2 => Some(1 << 1),
        3 => Some(1 << 2),
        4 => Some(1 << 3),
        5 => Some(1 << 4),
        QUERY_OPCODE => Some(1 << 5),
        COMMAND_OPCODE => Some(1 << 6),
        super::UPDATE_PORTABLE_METADATA_OPCODE => Some(1 << 7),
        _ => None,
    }
}

/// Every read-only history query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryQuery {
    /// One stack by identity.
    GetStack {
        /// Stack identity.
        stack: [u8; STACK_BYTES],
    },
    /// Stacks in name order.
    ListStacks {
        /// Continuation from a previous page; empty for the first page.
        cursor: Vec<u8>,
        /// Records requested.
        limit: u16,
    },
    /// One Branch by identity.
    GetBranch {
        /// Branch identity.
        branch: [u8; BRANCH_BYTES],
    },
    /// One stack's Branches in name order.
    ListBranches {
        /// Owning stack.
        stack: [u8; STACK_BYTES],
        /// Continuation from a previous page; empty for the first page.
        cursor: Vec<u8>,
        /// Records requested.
        limit: u16,
    },
    /// One Commit by identity.
    GetCommit {
        /// Commit identity.
        commit: [u8; COMMIT_BYTES],
    },
    /// One Branch's ancestry, newest first.
    CommitHistory {
        /// Branch whose ancestry is walked.
        branch: [u8; BRANCH_BYTES],
        /// Immutable starting Commit; absent means the live head.
        start: Option<[u8; COMMIT_BYTES]>,
        /// Continuation from a previous page; empty for the first page.
        cursor: Vec<u8>,
        /// Records requested.
        limit: u16,
    },
    /// One Layer by identity.
    GetLayer {
        /// Layer identity.
        layer: [u8; LAYER_BYTES],
    },
    /// One stack's publication chain, newest first.
    LayerHistory {
        /// Stack whose publications are walked.
        stack: [u8; STACK_BYTES],
        /// Immutable starting Layer; absent means the live head.
        start: Option<[u8; LAYER_BYTES]>,
        /// Continuation from a previous page; empty for the first page.
        cursor: Vec<u8>,
        /// Records requested.
        limit: u16,
    },
    /// One stage by producer incarnation.
    GetStage {
        /// Workspace incarnation.
        workspace: [u8; WORKSPACE_BYTES],
    },
    /// One Branch's stages in token order.
    ListStages {
        /// Branch whose stages are listed.
        branch: [u8; BRANCH_BYTES],
        /// Continuation from a previous page; empty for the first page.
        cursor: Vec<u8>,
        /// Records requested.
        limit: u16,
    },
}

/// Where a forked Branch starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryForkSource {
    /// A published Layer.
    Layer([u8; LAYER_BYTES]),
    /// A selected ancestor Commit reached through one Branch.
    Commit {
        /// Branch whose history is read.
        branch: [u8; BRANCH_BYTES],
        /// Selected ancestor Commit.
        commit: [u8; COMMIT_BYTES],
    },
}

/// One prepared filesystem update plus the Branch context it was captured in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedChanges {
    /// Producer incarnation that will own the stage.
    pub workspace: [u8; WORKSPACE_BYTES],
    /// Branch the candidate was constructed against.
    pub branch: [u8; BRANCH_BYTES],
    /// Head Commit captured before construction; absent for a Branch with none.
    pub expected_head: Option<[u8; COMMIT_BYTES]>,
    /// Base Layer captured before construction.
    pub expected_base: [u8; LAYER_BYTES],
    /// Producer generation this candidate was built from.
    pub generation: u64,
    /// Root C1 constructs from; it must equal the captured effective root.
    pub base: Root,
    /// Allocation scope of the prepared update.
    pub scope: Root,
    /// Root directory serial of the prepared update.
    pub root_serial: u64,
    /// Final directory bindings.
    pub directories: Vec<super::DirectoryChange>,
    /// Typed final inode values.
    pub inodes: Vec<super::InodeChange>,
}

/// One pathless manifest entry of a bounded namespace initialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestEntry {
    /// Index of the parent entry; zero for the root entry itself.
    pub parent: u16,
    /// Canonical component name; empty only for the root entry.
    pub name: Vec<u8>,
    /// Entry kind: 1 directory, 2 regular file, 3 symlink.
    pub kind: u8,
    /// Portable permission bits.
    pub mode: u32,
    /// Portable modification time, seconds since the Unix epoch.
    pub mtime_seconds: i64,
    /// Portable modification time, fractional second.
    pub mtime_nanoseconds: u32,
    /// Already published file root; regular files only.
    pub content: Option<Root>,
    /// Inline symbolic-link target; symlinks only.
    pub target: Vec<u8>,
}

/// Every mutating history command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryCommand {
    /// Creates the genesis Layer and the stack.
    InitLayerStack {
        /// Authority-supplied 16-byte stack body.
        stack: [u8; 16],
        /// Authority-local unique stack name.
        name: Vec<u8>,
        /// Seed the service derives the fresh allocation scope from.
        scope_seed: Root,
        /// Bounded logical namespace to build.
        manifest: Vec<ManifestEntry>,
    },
    /// Creates one Branch sharing the selected ancestry.
    Fork {
        /// Stack the new Branch belongs to.
        stack: [u8; STACK_BYTES],
        /// Authority-supplied 16-byte Branch body.
        branch: [u8; 16],
        /// Authority-local unique Branch name.
        name: Vec<u8>,
        /// Selected starting point.
        source: HistoryForkSource,
    },
    /// Saves the prepared update and freezes one exact stage.
    StageChanges(PreparedChanges),
    /// Commits one exact stage token.
    CommitStaged {
        /// Workspace incarnation that owns the stage.
        workspace: [u8; WORKSPACE_BYTES],
        /// Exact token the caller holds.
        token: u64,
    },
    /// Saves the prepared update and commits the frozen stage in one admission.
    Commit(PreparedChanges),
    /// Publishes one Commit root as a new Layer.
    AddLayer {
        /// Stack that receives the Layer.
        stack: [u8; STACK_BYTES],
        /// Branch that published the Commit.
        branch: [u8; BRANCH_BYTES],
        /// Selected source Commit.
        commit: [u8; COMMIT_BYTES],
        /// Stack head the caller expects.
        expected_stack_head: [u8; LAYER_BYTES],
        /// Branch base the caller expects.
        expected_branch_base: [u8; LAYER_BYTES],
    },
    /// Removes exactly one stage token.
    DiscardStage {
        /// Workspace incarnation that owns the stage.
        workspace: [u8; WORKSPACE_BYTES],
        /// Exact token the caller holds.
        token: u64,
    },
    /// Consumes one checked half-open inode range.
    ReserveInodes {
        /// Allocation scope that owns the serials.
        scope: Root,
        /// Serials requested.
        count: u64,
    },
}

/// One LayerStack as the wire carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackWire {
    /// Stack identity.
    pub stack: [u8; STACK_BYTES],
    /// Authority-local name.
    pub name: Vec<u8>,
    /// Allocation scope.
    pub scope: Root,
    /// Frozen filesystem profile.
    pub profile: Root,
    /// Current head Layer.
    pub head_layer: [u8; LAYER_BYTES],
}

/// One Branch as the wire carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchWire {
    /// Branch identity.
    pub branch: [u8; BRANCH_BYTES],
    /// Owning stack.
    pub stack: [u8; STACK_BYTES],
    /// Authority-local name.
    pub name: Vec<u8>,
    /// Base Layer.
    pub base_layer: [u8; LAYER_BYTES],
    /// Head Commit, absent for a freshly forked Branch.
    pub head_commit: Option<[u8; COMMIT_BYTES]>,
}

/// One coherent Branch snapshot with its resolved roots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchSnapshotWire {
    /// Branch metadata.
    pub branch: BranchWire,
    /// Head Commit root, absent when the Branch has no Commit.
    pub head_root: Option<Root>,
    /// Base Layer root.
    pub base_root: Root,
    /// Head root when present, otherwise the base root.
    pub effective_root: Root,
    /// Validated for GetBranch; absent on a metadata-only Fork snapshot.
    pub root_serial: Option<u64>,
    /// Allocation scope.
    pub scope: Root,
    /// Frozen filesystem profile.
    pub profile: Root,
}

/// One Commit as the wire carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitWire {
    /// Commit identity.
    pub commit: [u8; COMMIT_BYTES],
    /// Owning stack.
    pub stack: [u8; STACK_BYTES],
    /// Complete filesystem root.
    pub root: Root,
    /// Recorded ancestry.
    pub parent: Option<[u8; COMMIT_BYTES]>,
    /// Base Layer.
    pub base_layer: [u8; LAYER_BYTES],
}

/// One Layer as the wire carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerWire {
    /// Layer identity.
    pub layer: [u8; LAYER_BYTES],
    /// Owning stack.
    pub stack: [u8; STACK_BYTES],
    /// Previous publication; absent only for genesis.
    pub parent: Option<[u8; LAYER_BYTES]>,
    /// Complete filesystem root.
    pub root: Root,
    /// Publishing Branch; absent only for genesis.
    pub source_branch: Option<[u8; BRANCH_BYTES]>,
    /// Published Commit; absent only for genesis.
    pub source_commit: Option<[u8; COMMIT_BYTES]>,
}

/// One frozen Workspace stage as the wire carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageWire {
    /// Producer incarnation.
    pub workspace: [u8; WORKSPACE_BYTES],
    /// Exact token.
    pub token: u64,
    /// Captured stack.
    pub stack: [u8; STACK_BYTES],
    /// Captured Branch.
    pub branch: [u8; BRANCH_BYTES],
    /// Captured head Commit.
    pub expected_head: Option<[u8; COMMIT_BYTES]>,
    /// Captured base Layer.
    pub expected_base: [u8; LAYER_BYTES],
    /// Captured effective root.
    pub expected_root: Root,
    /// Root the candidate was constructed from.
    pub construction_base_root: Root,
    /// Base Layer the Commit would record.
    pub intended_commit_base: [u8; LAYER_BYTES],
    /// Saved root of the candidate.
    pub candidate_root: Root,
    /// Frozen filesystem profile.
    pub profile: Root,
    /// Allocation scope.
    pub scope: Root,
    /// Producer generation.
    pub generation: u64,
}

/// Result of committing one exact stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitOutcomeWire {
    /// A new immutable Commit was inserted and the Branch advanced.
    Committed(CommitWire),
    /// The candidate root and intended base already describe the Branch.
    UpToDate {
        /// Head Commit, absent when the Branch has none.
        head: Option<[u8; COMMIT_BYTES]>,
        /// Effective root the Branch already has.
        root: Root,
    },
}

/// Result of a Layer publication attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayerOutcomeWire {
    /// A new immutable Layer was inserted and the stack advanced.
    Added(LayerWire),
    /// The selected source Commit already published in this stack.
    UpToDate {
        /// Layer that already publishes this source.
        layer: [u8; LAYER_BYTES],
    },
    /// The Commit root equals its base Layer root.
    NoChanges {
        /// Stack head, unchanged.
        head: [u8; LAYER_BYTES],
    },
}

/// Every history reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryResult {
    /// One stack.
    Stack(StackWire),
    /// One page of stacks.
    Stacks {
        /// Continuation when more records remain.
        continuation: Vec<u8>,
        /// Records in query order.
        records: Vec<StackWire>,
    },
    /// One page of Branches.
    Branches {
        /// Continuation when more records remain.
        continuation: Vec<u8>,
        /// Records in query order.
        records: Vec<BranchWire>,
    },
    /// One coherent Branch snapshot: the answer to reading one Branch.
    BranchSnapshot(BranchSnapshotWire),
    /// One Commit.
    Commit(CommitWire),
    /// One page of Commits.
    Commits {
        /// Continuation when more records remain.
        continuation: Vec<u8>,
        /// Records in query order.
        records: Vec<CommitWire>,
    },
    /// One Layer.
    Layer(LayerWire),
    /// One page of Layers.
    Layers {
        /// Continuation when more records remain.
        continuation: Vec<u8>,
        /// Records in query order.
        records: Vec<LayerWire>,
    },
    /// One stage.
    Stage(StageWire),
    /// One page of stages.
    Stages {
        /// Continuation when more records remain.
        continuation: Vec<u8>,
        /// Records in query order.
        records: Vec<StageWire>,
    },
    /// The stack created by an initialization.
    StackCreated(StackCreatedWire),
    /// The outcome of committing one exact stage.
    Committed(CommitOutcomeWire),
    /// The outcome of publishing one Layer.
    Published(LayerOutcomeWire),
    /// The outcome of removing one exact stage.
    Discarded {
        /// True when the exact stage existed and was removed.
        removed: bool,
    },
    /// One consumed inode reservation.
    Reservation {
        /// Scope the serials belong to.
        scope: Root,
        /// First serial of the reservation.
        start: u64,
        /// Serials reserved.
        count: u64,
    },
}

/// Typed conflict context from the deciding snapshot/transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryConflict {
    BranchMoved {
        expected_head: Option<[u8; 33]>,
        actual_head: Option<[u8; 33]>,
        expected_base: [u8; 33],
        actual_base: [u8; 33],
    },
    StackMoved {
        expected: [u8; 33],
        actual: [u8; 33],
    },
    StageChanged {
        expected: u64,
        actual: Option<u64>,
    },
    BaseMismatch {
        commit_base: [u8; 33],
        branch_base: [u8; 33],
    },
}

/// Exact stage disposition; lack of observation is not absence.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum StageObservation {
    #[default]
    Unobserved,
    Absent([u8; 32]),
    Retained(Box<StageWire>),
    AcknowledgedUnknown(Box<StageWire>),
}

/// History-only failure payload, boxed to keep ordinary failures small.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct HistoryFailure {
    pub conflict: Option<HistoryConflict>,
    pub stage: StageObservation,
}

/// Initialization descriptor known from successful C1 construction and C2 finish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackCreatedWire {
    pub stack: StackWire,
    pub root: Root,
    pub root_serial: u64,
}

pub(crate) fn tag(bytes: &[u8], expected: u8) -> Result<(), Failure> {
    if bytes.first() != Some(&expected) {
        return Err(Code::InvalidInput.into());
    }
    Ok(())
}
pub(crate) fn serial(value: u64) -> Result<(), Failure> {
    if value == 0 || value > i64::MAX as u64 {
        return Err(Code::InvalidInput.into());
    }
    Ok(())
}

pub(crate) fn check_commit(record: &CommitWire) -> Result<(), Failure> {
    tag(&record.commit, 0x12)?;
    tag(&record.stack, 0x31)?;
    tag(&record.base_layer, 0x32)?;
    if let Some(parent) = record.parent {
        tag(&parent, 0x12)?;
    }
    Ok(())
}

pub(crate) fn check_stage(record: &StageWire) -> Result<(), Failure> {
    if record.workspace == [0; 32]
        || record.generation > i64::MAX as u64
        || record.construction_base_root != record.expected_root
        || record.intended_commit_base != record.expected_base
    {
        return Err(Code::InvalidInput.into());
    }
    serial(record.token)?;
    tag(&record.stack, 0x31)?;
    tag(&record.branch, 0x11)?;
    tag(&record.expected_base, 0x32)?;
    tag(&record.intended_commit_base, 0x32)?;
    if let Some(head) = record.expected_head {
        tag(&head, 0x12)?;
    }
    Ok(())
}
