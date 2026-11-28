//! The small history catalog contract every provider implements.
//!
//! The contract is semantic: each method is one named history operation over
//! typed values, never a SQL string, a table name, a row cursor or a callback
//! that hands a caller the database.
//!
//! That is what keeps C5's transitions atomic - the caller states the intent and
//! the provider owns the transaction - and it is why a second provider could be
//! added later without reopening this crate's callers.
//!
//! Two rules hold for every implementation:
//!
//! - a metadata transaction is short and never spans a caller's upload, C1
//!   construction or C2 save completion; the provider takes its own admission
//!   and returns, and
//! - a refusal is explicit. Contention is [`HistoryError::Busy`], not a
//!   semantic conflict and not a silent retry.

use crate::error::HistoryResult;
use crate::identity::{BranchId, CatalogId, CommitId, HistoryName, LayerId, LayerStackId, NodeId};
use crate::records::{
    AddLayerOutcome, AddLayerRequest, BranchMergeRequest, BranchRecord, BranchSnapshot,
    CommitHistoryRequest, CommitOutcome, CommitRecord, CommitRequest, ForkRequest, GcAuditAfter,
    GcAuditPage, GcBranchAuditPage, GcDeletion, GcIntent, GcReconcile, GcScopeKind,
    LayerHistoryRequest, LayerRecord, LayerStackRecord, Page, PageResult, PathRemovalOutcome,
    PathRemovalRecord, PathRemovalRequest, Reservation, ReserveRequest, StackInitialization,
};
/// Checked inputs that establish one catalog binding.
#[derive(Clone, Eq, PartialEq)]
pub struct HistoryCatalogConfig {
    /// Stable authority binding key. It identifies the content authority this
    /// catalog is bound to; the caller is responsible for having selected the
    /// right physical database for it.
    pub binding_key: Vec<u8>,
    /// Catalog incarnation. A fresh creation states it; a cursor binds it, so a
    /// cursor from a recreated catalog cannot be replayed into this one.
    pub incarnation: u64,
    /// Authority-supplied secret for cursor authentication; retained outside the catalog.
    pub cursor_key: [u8; 32],
}

impl HistoryCatalogConfig {
    /// Checks both inputs.
    pub fn check(&self) -> HistoryResult<()> {
        CatalogId::derive(&self.binding_key)?;
        if self.cursor_key == [0; 32] {
            return Err(crate::error::HistoryError::InvalidInput(
                "cursor capability",
            ));
        }
        if self.incarnation == 0 || self.incarnation > i64::MAX as u64 {
            return Err(crate::error::HistoryError::InvalidInput(
                "catalog incarnation",
            ));
        }
        Ok(())
    }
}

/// A Branch snapshot found by its stack's name and its own
/// ([`HistoryCatalog::branch_snapshot_named`]).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NamedSnapshot {
    /// No stack has that name.
    NoStack,
    /// The stack has no Branch of that name.
    NoBranch,
    /// The Branch's coherent snapshot.
    Found(Box<BranchSnapshot>),
}

/// The saved snapshot a read names ([`HistoryCatalog::read_context`]).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextTarget {
    /// The stack's head Layer.
    Main,
    /// A Branch by its name: its head Commit, else its base Layer.
    Branch(HistoryName),
    /// A Commit or Layer by its full identity.
    Node(NodeId),
}

/// A saved node's record: a Commit or a Layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeRecord {
    /// A Commit.
    Commit(Box<CommitRecord>),
    /// A Layer publication.
    Layer(LayerRecord),
}

impl NodeRecord {
    /// The project the node belongs to.
    pub fn stack(&self) -> LayerStackId {
        match self {
            Self::Commit(record) => record.stack,
            Self::Layer(record) => record.stack,
        }
    }

    /// The root the node records.
    pub fn root(&self) -> layerfs_content::ObjectId {
        match self {
            Self::Commit(record) => record.root,
            Self::Layer(record) => record.root,
        }
    }
}

/// A node's `history_serial` with its stack's `path_removed` markers, or
/// `None` when the node has no row ([`HistoryCatalog::node_path_removals`]).
pub type NodeRemovals = Option<(u64, Vec<PathRemovalRecord>)>;

/// What one read call needs from history before its path, found by its
/// project's name and the snapshot it names ([`HistoryCatalog::read_context`]).
/// Each part is `None` where the read stopped: no stack ends the read, no
/// Branch ends it after the stack, and a missing node row leaves `node`
/// `None`. `removals` and `recorded` are `None` when not read here; the
/// caller then reads them itself, as it did before (#818 R5). Each is its
/// own result: a part read here but not decoded is that part's error, which
/// the caller meets where it read the part before, after its other checks.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReadContext {
    /// The project's stack.
    pub stack: Option<LayerStackRecord>,
    /// The named Branch (a [`ContextTarget::Branch`] only).
    pub branch: Option<BranchRecord>,
    /// The named node's record.
    pub node: Option<NodeRecord>,
    /// The node's `history_serial` with the stack's `path_removed` markers,
    /// as [`HistoryCatalog::node_path_removals`] reads them.
    pub removals: Option<HistoryResult<NodeRemovals>>,
    /// A Commit's recorded Branch, as [`HistoryCatalog::branch_any`] reads
    /// it; `Some(None)` when it records none or its row is gone.
    pub recorded: Option<HistoryResult<Option<BranchRecord>>>,
}

/// [`HistoryCatalog::read_context`]'s default: the stack, the Branch and the
/// node's record, read one after another in that order, each read only when
/// the one before found its row. It reads neither the markers nor the
/// recorded Branch: the caller reads them where it always did, so a provider
/// without the one statement sends the same statements in the same order as
/// before, refusals included (#818 R5).
pub fn read_context_serial<C: HistoryCatalog + ?Sized>(
    catalog: &C,
    project: &HistoryName,
    target: &ContextTarget,
) -> HistoryResult<ReadContext> {
    let mut context = ReadContext::default();
    let Some(stack) = catalog.layer_stack_named(project)? else {
        return Ok(context);
    };
    let node = match target {
        ContextTarget::Main => NodeId::Layer(stack.head_layer),
        ContextTarget::Node(node) => *node,
        ContextTarget::Branch(name) => {
            let found = catalog.branch_named(stack.id, name)?;
            let Some(branch) = found else {
                context.stack = Some(stack);
                return Ok(context);
            };
            let node = branch
                .head_commit
                .map_or(NodeId::Layer(branch.base_layer), NodeId::Commit);
            context.branch = Some(branch);
            node
        }
    };
    context.stack = Some(stack);
    context.node = match node {
        NodeId::Commit(id) => catalog
            .commit(id)?
            .map(|record| NodeRecord::Commit(Box::new(record))),
        NodeId::Layer(id) => catalog.layer(id)?.map(NodeRecord::Layer),
    };
    Ok(context)
}

/// One Commit of a first-parent window ([`HistoryCatalog::first_parent_window`]):
/// the Commit's record and the `history_serial` it records, as
/// [`HistoryCatalog::commit`] and [`HistoryCatalog::node_history_serial`]
/// read them (a provider that keeps no serial gives 0).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChainRow {
    /// The Commit's record.
    pub record: CommitRecord,
    /// Its `history_serial`.
    pub serial: u64,
}

/// [`HistoryCatalog::first_parent_window`]'s default: from `head`, each
/// Commit's record and then its serial, then its first parent, until
/// `count` rows, a Commit without a row, or a Commit whose parent is its
/// base Layer. A row that fails ends the window with its error, so a caller
/// meets the failure only on reaching that row (#818 H4).
pub fn first_parent_window_serial<C: HistoryCatalog + ?Sized>(
    catalog: &C,
    head: CommitId,
    count: usize,
) -> HistoryResult<Vec<HistoryResult<ChainRow>>> {
    let mut rows = Vec::new();
    let mut next = Some(head);
    while let Some(id) = next.filter(|_| rows.len() < count) {
        let row = catalog.commit(id).and_then(|record| {
            let Some(record) = record else {
                return Ok(None);
            };
            let serial = match catalog.node_history_serial(NodeId::Commit(id)) {
                Ok(serial) => serial.unwrap_or(0),
                Err(crate::error::HistoryError::Unsupported(_)) => 0,
                Err(error) => return Err(error),
            };
            Ok(Some(ChainRow { record, serial }))
        });
        match row {
            Ok(None) => break,
            Ok(Some(row)) => {
                next = row.record.parent;
                rows.push(Ok(row));
            }
            Err(error) => {
                rows.push(Err(error));
                break;
            }
        }
    }
    Ok(rows)
}

/// What a mount's registration read beside its intent
/// ([`HistoryCatalog::gc_register_mount`]).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MountMarkers {
    /// Nothing: the caller reads the bound node's removals itself, after the
    /// registration, as [`HistoryCatalog::node_path_removals`].
    NotRead,
    /// The bound node's [`HistoryCatalog::node_path_removals`], read after
    /// the intent committed, on the same connection.
    Read(Option<(u64, Vec<PathRemovalRecord>)>),
}

/// Every semantic history operation, portable over its provider.
pub trait HistoryCatalog: Send + Sync {
    /// Identity this catalog was created or opened with.
    fn catalog_id(&self) -> CatalogId;

    /// Incarnation recorded when this catalog was created.
    fn incarnation(&self) -> u64;

    /// Authenticates an opaque saved-query continuation with this catalog's
    /// authority capability. The key is never exported; the digest binds the
    /// catalog identity and incarnation as well as the bounded payload.
    fn query_authenticator(&self, _payload: &[u8]) -> HistoryResult<[u8; 32]> {
        Err(crate::error::HistoryError::Unsupported(
            "saved-query continuation authentication",
        ))
    }

    /// One stack by identity.
    fn layer_stack(&self, id: LayerStackId) -> HistoryResult<Option<LayerStackRecord>>;

    /// Stacks in name order.
    fn layer_stacks(&self, page: &Page) -> HistoryResult<PageResult<LayerStackRecord>>;

    /// The typed `NameTaken` state for a LayerStack (project) name, live or
    /// deleted, read without resolving the Stack (spec 4.4). The default
    /// reports none; a provider that keeps the collision query overrides it,
    /// so `project.init` refuses a held name before it scans anything.
    fn layer_stack_name_taken(
        &self,
        _name: &HistoryName,
    ) -> HistoryResult<Option<crate::error::NameTakenState>> {
        Ok(None)
    }

    /// The stack named `name`, if any. A provider answers through its name
    /// index; this default pages through the stacks in name order.
    fn layer_stack_named(&self, name: &HistoryName) -> HistoryResult<Option<LayerStackRecord>> {
        let mut page = Page {
            cursor: None,
            limit: crate::records::MAXIMUM_PAGE_RECORDS,
        };
        loop {
            let found = self.layer_stacks(&page)?;
            if let Some(stack) = found.records.into_iter().find(|s| &s.name == name) {
                return Ok(Some(stack));
            }
            match found.continuation {
                Some(cursor) => page.cursor = Some(cursor),
                None => return Ok(None),
            }
        }
    }

    /// One Branch by identity.
    fn branch(&self, id: BranchId) -> HistoryResult<Option<BranchRecord>>;

    /// One Branch by identity without the visibility filter (spec 4.2). Only
    /// rendering and identity keep a retired Branch's name this way; every
    /// caller-facing lookup resolves through [`HistoryCatalog::branch`]. A
    /// provider that keeps retired rows overrides this; the default reads the
    /// filtered row.
    fn branch_any(&self, id: BranchId) -> HistoryResult<Option<BranchRecord>> {
        self.branch(id)
    }

    /// One Branch by its name in a stack, through the name index: the
    /// stack's other Branches are not read.
    fn branch_named(
        &self,
        stack: LayerStackId,
        name: &HistoryName,
    ) -> HistoryResult<Option<BranchRecord>>;

    /// One coherent Branch snapshot with its resolved roots.
    fn branch_snapshot(&self, id: BranchId) -> HistoryResult<Option<BranchSnapshot>>;

    /// The snapshot of Branch `branch` of the stack named `stack`, both found
    /// through their name indexes. A provider answers in one statement; this
    /// default reads the stack, the Branch and the snapshot one after another.
    fn branch_snapshot_named(
        &self,
        stack: &HistoryName,
        branch: &HistoryName,
    ) -> HistoryResult<NamedSnapshot> {
        let Some(found) = self.layer_stack_named(stack)? else {
            return Ok(NamedSnapshot::NoStack);
        };
        let Some(record) = self.branch_named(found.id, branch)? else {
            return Ok(NamedSnapshot::NoBranch);
        };
        Ok(match self.branch_snapshot(record.id)? {
            Some(snapshot) => NamedSnapshot::Found(Box::new(snapshot)),
            None => NamedSnapshot::NoBranch,
        })
    }

    /// What a read call needs before its path, for project `project` and the
    /// snapshot `target` names ([`ReadContext`]). A provider answers in one
    /// statement, markers and recorded Branch included; this default is
    /// [`read_context_serial`].
    fn read_context(
        &self,
        project: &HistoryName,
        target: &ContextTarget,
    ) -> HistoryResult<ReadContext> {
        read_context_serial(self, project, target)
    }

    /// Up to `count` Commits along first parents from `head`, newest first
    /// ([`ChainRow`]); the window ends early at a Commit whose parent is its
    /// base Layer or has no row. Each row is its own result: a row that
    /// fails ends the window, and an error is the caller's only where its
    /// walk reaches that row. A provider answers in one statement; this
    /// default is [`first_parent_window_serial`] (#818 H4).
    fn first_parent_window(
        &self,
        head: CommitId,
        count: usize,
    ) -> HistoryResult<Vec<HistoryResult<ChainRow>>> {
        first_parent_window_serial(self, head, count)
    }

    /// [`Self::branch_snapshot`] with the name of the Branch's stack. A
    /// provider answers from the snapshot's own statement, which already
    /// reads the stack row; this default reads the stack after the snapshot.
    fn branch_snapshot_with_stack_name(
        &self,
        id: BranchId,
    ) -> HistoryResult<Option<(BranchSnapshot, HistoryName)>> {
        let Some(snapshot) = self.branch_snapshot(id)? else {
            return Ok(None);
        };
        let stack = self
            .layer_stack(snapshot.branch.stack)?
            .ok_or(crate::error::HistoryError::Integrity("Branch LayerStack"))?;
        Ok(Some((snapshot, stack.name)))
    }

    /// Branches of one stack in name order.
    fn branches(&self, stack: LayerStackId, page: &Page)
        -> HistoryResult<PageResult<BranchRecord>>;

    /// Branches of one stack in name order from the name `from` on: the names
    /// after it, and `from` itself when `inclusive`. At most `limit` records
    /// (one to 128), read through the name index: a list filtered by a name
    /// prefix, or continued after the last name of a page, reads only its own
    /// rows. The default refuses.
    fn branches_from(
        &self,
        stack: LayerStackId,
        from: &str,
        inclusive: bool,
        limit: u16,
    ) -> HistoryResult<Vec<BranchRecord>> {
        let _ = (stack, from, inclusive, limit);
        Err(crate::error::HistoryError::Unsupported("Branch name range"))
    }

    /// The nodes of one stack whose tagged identity lies in `low..=high`,
    /// both of one kind (the same leading tag): the first of them in identity
    /// order and how many there are. One bounded read through the stack's
    /// identity index, so a caller resolves a prefix of an identity without
    /// paging history. The default refuses.
    fn nodes_between(
        &self,
        stack: LayerStackId,
        low: &[u8; 33],
        high: &[u8; 33],
    ) -> HistoryResult<(Option<NodeId>, u64)> {
        let _ = (stack, low, high);
        Err(crate::error::HistoryError::Unsupported(
            "identity prefix lookup",
        ))
    }

    /// One Commit by identity.
    fn commit(&self, id: CommitId) -> HistoryResult<Option<CommitRecord>>;

    /// The Commit the publication of `attempt` made, if one did and it is
    /// still stored (#776 R1). One indexed read; the default refuses.
    fn commit_by_attempt(
        &self,
        attempt: crate::identity::CommitAttempt,
    ) -> HistoryResult<Option<CommitRecord>> {
        let _ = attempt;
        Err(crate::error::HistoryError::Unsupported(
            "Commit attempt lookup",
        ))
    }

    /// One Layer by identity.
    fn layer(&self, id: LayerId) -> HistoryResult<Option<LayerRecord>>;

    /// Ancestry walk of one Branch, newest first, one bounded page at a time.
    fn commit_history(
        &self,
        request: &CommitHistoryRequest,
    ) -> HistoryResult<PageResult<CommitRecord>>;

    /// Publication chain of one stack, newest first, one bounded page at a time.
    fn layer_history(
        &self,
        request: &LayerHistoryRequest,
    ) -> HistoryResult<PageResult<LayerRecord>>;

    /// The newest common ancestor of two history nodes, through Commit parents
    /// (including second parents), base Layers, Layer parents and the Commit
    /// each Layer was published from. Absent when the two share no ancestor.
    /// The answer is exact and has no history-size bound: the work follows the
    /// merges and runs the sides made since the Layers that absorbed them, not
    /// the number of Commits or Layers. Among several maximal common
    /// ancestors (criss-cross) the one with the newest Layer in its ancestry
    /// wins, then the deepest first parent, then the smallest identity.
    fn merge_base(&self, left: NodeId, right: NodeId) -> HistoryResult<Option<NodeId>>;

    /// True when `ancestor` equals `descendant` or is reachable from it over
    /// the same edges as [`HistoryCatalog::merge_base`].
    fn is_ancestor(&self, ancestor: NodeId, descendant: NodeId) -> HistoryResult<bool>;

    /// The Layers a publication overwrote, newest first: up to `limit` of
    /// them after skipping `position`. Each page is one bounded read,
    /// whatever the list's length.
    fn overwritten(
        &self,
        layer: LayerId,
        position: u64,
        limit: u16,
    ) -> HistoryResult<Vec<LayerId>> {
        let _ = (layer, position, limit);
        Err(crate::error::HistoryError::Unsupported(
            "overwritten Layers",
        ))
    }

    /// The Layer `commit` of `branch` was published as, if it was: the same
    /// record `add_layer` answers `UpToDate` from. One bounded read.
    fn published_layer(
        &self,
        branch: BranchId,
        commit: CommitId,
    ) -> HistoryResult<Option<LayerId>> {
        let _ = (branch, commit);
        Err(crate::error::HistoryError::Unsupported("published Layer"))
    }

    /// Reads maintained Store totals; no full object/history scan.
    fn statistics(&self) -> HistoryResult<crate::records::StoreStatistics> {
        Err(crate::HistoryError::Unsupported("Store statistics"))
    }

    /// Records Init's totals once; a second record is `Integrity`.
    fn record_init(&self, _totals: &crate::records::InitStatistics) -> HistoryResult<()> {
        Err(crate::HistoryError::Unsupported("Init statistics"))
    }

    /// Records name bindings saved trees hold, so a reader can find an
    /// inode's paths without walking a tree. A recorded row stays; recording
    /// it again is not an error. A provider that keeps none accepts and drops
    /// them, and its readers walk instead.
    fn record_bindings(&self, _rows: &[crate::records::BindingHint]) -> HistoryResult<()> {
        Ok(())
    }

    /// Appends one overflow batch after the Commit's Save has acknowledged.
    /// Each batch acknowledges independently before the caller sends the next.
    fn record_overflow_bindings(&self, rows: &[crate::records::BindingHint]) -> HistoryResult<()> {
        self.record_bindings(rows)
    }

    /// Every recorded binding of inode `child`, from any saved tree: hints the
    /// caller checks against its own tree. Empty when none are kept.
    fn bindings_of(&self, _child: u64) -> HistoryResult<Vec<crate::records::BindingHint>> {
        Ok(Vec::new())
    }

    /// Records whole-content digests of saved file roots (#364). Recorded
    /// content takes the newer root. A provider that keeps none drops them.
    fn record_file_digests(&self, _rows: &[crate::records::FileDigest]) -> HistoryResult<()> {
        Ok(())
    }

    /// The recorded root of each of at most
    /// [`crate::records::FILE_DIGEST_WINDOW`] (digest, length) pairs, in the
    /// order asked; `None` where none is recorded.
    fn file_roots(&self, contents: &[([u8; 32], u64)]) -> HistoryResult<Vec<Option<[u8; 32]>>> {
        Ok(vec![None; contents.len()])
    }

    /// Creates the genesis Layer and the stack atomically.
    fn initialize_layerstack(
        &self,
        request: &StackInitialization,
    ) -> HistoryResult<LayerStackRecord>;

    /// Creates one Branch sharing the selected ancestry.
    fn fork(&self, request: &ForkRequest) -> HistoryResult<BranchSnapshot>;

    /// Removes one LayerStack (a project) in one write transaction: its
    /// Layers, Branches, Commits and their lineage, and their logical bytes
    /// from the Store totals. Store-wide packs, objects and file digests stay,
    /// and so does the stack's allocator scope; reclaiming packs no stack
    /// reaches is separate (#411). An unknown stack is `Missing(LayerStack)`.
    /// A provider that cannot remove stacks refuses as `Unsupported`.
    fn remove_layerstack(&self, _stack: LayerStackId) -> HistoryResult<()> {
        Err(crate::error::HistoryError::Unsupported(
            "LayerStack removal",
        ))
    }

    /// Holds the Store's publication lane for the calling thread until the
    /// returned value is dropped: no other publication, in this process or
    /// another, can interleave with the caller's read of a head and its
    /// publication against it. The caller's own history calls run inside it.
    /// A provider with no concurrent publishers returns an empty hold.
    fn publish_lane(&self) -> HistoryResult<PublishLane> {
        Ok(PublishLane(None))
    }

    /// [`Self::publish_lane`], and whether Branch `branch` is visible at head
    /// Commit `head` over base Layer `base` once the lane is held. `true`
    /// only when the provider read that in the same step as taking the lane;
    /// `false` when it did not read (the default) or found anything else, and
    /// the caller then reads the Branch itself.
    fn publish_lane_unmoved(
        &self,
        _branch: BranchId,
        _head: Option<CommitId>,
        _base: LayerId,
    ) -> HistoryResult<(PublishLane, bool)> {
        Ok((self.publish_lane()?, false))
    }

    /// Returns once every publication that held this Store's lane when the
    /// call began has ended, committed or rolled back, on a connection of its
    /// own: a lost `COMMIT` reply may leave the server still applying it, and
    /// nothing may judge that publication's outcome before then (#776 C2). It
    /// takes and releases the lane, writes nothing and keeps nothing; a wait
    /// that reaches the provider's lock timeout is `Busy`. A provider whose
    /// lost publication may still run outside the lane (a PostgreSQL message
    /// that sends its packs before the lane row) first ends that work. The default returns
    /// at once: a provider whose publications cannot complete after their
    /// caller gave up (an in-process SQLite file, a test double) has none.
    fn pass_publication_lane(&self) -> HistoryResult<()> {
        Ok(())
    }

    /// Publishes one saved candidate as a Commit against the expected Branch
    /// head in one write transaction. A moved head is `HeadMoved` unless the
    /// request forces an overwrite. UpToDate compares the current root.
    /// Unknown retains the original request; nothing is reread or replayed.
    fn commit_branch(&self, request: &CommitRequest) -> HistoryResult<CommitOutcome>;

    /// Publishes one Commit root as a new Layer against the expected stack head.
    /// The Commit is the Branch's head or one its head holds.
    fn add_layer(&self, request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome>;

    /// Creates a Branch from a merge of two sides, atomically.
    fn merge_branch(&self, request: &BranchMergeRequest) -> HistoryResult<BranchSnapshot>;

    /// Moves an existing Branch to the result of merging another side into
    /// it, atomically: `request.branch` is the Branch (its `name` is not
    /// read), `sides[0]` is that Branch and the node it must still be at, and
    /// `sides[1]` the side merged in and the node it must still be at. A
    /// fast-forward moves the Branch to an existing node (a Commit with that
    /// Commit's base, or a Layer as its base with no head); a merge gives it
    /// one new Commit with two parents and that Commit's base. The other side
    /// does not change. The default refuses.
    fn merge_into_branch(&self, request: &BranchMergeRequest) -> HistoryResult<BranchSnapshot> {
        let _ = request;
        Err(crate::error::HistoryError::Unsupported(
            "merge into an existing Branch",
        ))
    }

    /// Consumes one checked half-open inode range for a scope.
    fn reserve_inodes(&self, request: &ReserveRequest) -> HistoryResult<Reservation>;

    /// Records a `branch.remove` marker. The default refuses.
    ///
    /// # Errors
    ///
    /// The default returns [`crate::error::HistoryError::Unsupported`]
    /// because it keeps no removal markers; an implementation returns its own
    /// [`crate::error::HistoryError`] when the marker cannot be recorded.
    fn mark_branch_removed(&self, _branch: BranchId, _removed_ms: i64) -> HistoryResult<()> {
        Err(crate::error::HistoryError::Unsupported(
            "branch removal marker",
        ))
    }

    /// Records a `project.delete` marker. The default refuses.
    ///
    /// # Errors
    ///
    /// The default returns [`crate::error::HistoryError::Unsupported`]
    /// because it keeps no deletion markers; an implementation returns its own
    /// [`crate::error::HistoryError`] when the marker cannot be recorded.
    fn mark_stack_deleted(&self, _stack: LayerStackId, _deleted_ms: i64) -> HistoryResult<()> {
        Err(crate::error::HistoryError::Unsupported(
            "project deletion marker",
        ))
    }

    /// The deleted_ms a project.delete marker recorded, when the stack is
    /// retired. A same-home retry reads it by the stack id its durable intent
    /// recorded before the marker; a live or absent stack has none. The
    /// default refuses.
    fn stack_deleted_ms(&self, _stack: LayerStackId) -> HistoryResult<Option<i64>> {
        Err(crate::error::HistoryError::Unsupported(
            "project deletion marker read",
        ))
    }

    /// Records a `project.hide_path` marker. The default refuses.
    ///
    /// # Errors
    ///
    /// The default returns [`crate::error::HistoryError::Unsupported`]
    /// because it keeps no path removal markers; an implementation returns its own
    /// [`crate::error::HistoryError`] when the marker cannot be recorded.
    fn mark_path_removed(
        &self,
        _stack: LayerStackId,
        _path: &[u8],
        _frontier_serial: u64,
        _removed_ms: i64,
    ) -> HistoryResult<()> {
        Err(crate::error::HistoryError::Unsupported(
            "path removal marker",
        ))
    }

    /// Applies one `project.hide_path` marker in one write transaction:
    /// reads the Store high-water as the new frontier, returns `UpToDate`
    /// when the path's marker already covers every project node, otherwise
    /// advances the marker and the project epoch. The default refuses.
    fn remove_path(&self, _request: &PathRemovalRequest) -> HistoryResult<PathRemovalOutcome> {
        Err(crate::error::HistoryError::Unsupported("path removal"))
    }

    /// Every `path_removed` marker of one Stack. The default refuses.
    fn path_removals(&self, _stack: LayerStackId) -> HistoryResult<Vec<PathRemovalRecord>> {
        Err(crate::error::HistoryError::Unsupported(
            "path removal markers",
        ))
    }

    /// The `history_serial` a Commit or Layer records, if it exists. The
    /// default refuses.
    fn node_history_serial(&self, _node: NodeId) -> HistoryResult<Option<u64>> {
        Err(crate::error::HistoryError::Unsupported(
            "node history position",
        ))
    }

    /// The `history_serial` `node` records with every `path_removed` marker
    /// of `stack`, or `None` when the node does not exist. A provider reads
    /// both in one statement, so they come from one snapshot; this default
    /// reads [`Self::node_history_serial`] and then [`Self::path_removals`],
    /// and refuses as they do.
    fn node_path_removals(
        &self,
        stack: LayerStackId,
        node: NodeId,
    ) -> HistoryResult<Option<(u64, Vec<PathRemovalRecord>)>> {
        let Some(serial) = self.node_history_serial(node)? else {
            return Ok(None);
        };
        Ok(Some((serial, self.path_removals(stack)?)))
    }

    /// The retained history roots of one Stack. The default refuses.
    ///
    /// # Errors
    ///
    /// The default returns [`crate::error::HistoryError::Unsupported`]
    /// because it retains no roots; an implementation returns its own
    /// [`crate::error::HistoryError`] when the roots cannot be read.
    fn retained_roots(&self, _stack: LayerStackId) -> HistoryResult<Vec<NodeId>> {
        Err(crate::error::HistoryError::Unsupported("retained roots"))
    }

    /// The retained history roots of one Stack, for a caller that resolves
    /// them separately. The default refuses.
    ///
    /// # Errors
    ///
    /// The default returns [`crate::error::HistoryError::Unsupported`]
    /// because it retains no roots; an implementation returns its own
    /// [`crate::error::HistoryError`] when the roots cannot be read.
    fn get_retained_roots(&self, _stack: LayerStackId) -> HistoryResult<Vec<NodeId>> {
        Err(crate::error::HistoryError::Unsupported("retained roots"))
    }

    /// Removes the unreachable history rows of one Stack, or reports how many
    /// a dry run would remove. The mark that decides reachability runs in the
    /// same write transaction as the removal (a dry run runs it too), so a
    /// node created after an earlier mark is never collected. The returned
    /// count covers that Stack's Commit, Layer, Branch and LayerStack rows;
    /// the orphaned lineage rows the cleanup also drops are not included. The
    /// default refuses.
    ///
    /// # Errors
    ///
    /// The default returns [`crate::error::HistoryError::Unsupported`]
    /// because it removes nothing; an implementation returns its own
    /// [`crate::error::HistoryError`] when the rows cannot be removed.
    fn remove_unreachable(&self, _stack: LayerStackId, _dry_run: bool) -> HistoryResult<u64> {
        Err(crate::error::HistoryError::Unsupported(
            "unreachable history removal",
        ))
    }

    /// Registers one admission intent (§5) in its own write transaction.
    ///
    /// The default refuses: a provider without admission cannot make the
    /// `MountsActive`/`Busy`/`Unresolved` promise.
    fn gc_register(&self, _intent: &GcIntent) -> HistoryResult<()> {
        Err(crate::error::HistoryError::Unsupported(
            crate::error::NO_ADMISSION,
        ))
    }

    /// Registers a Workspace mount's intent, as [`Self::gc_register`], and
    /// may read the bound `node`'s removals in `stack` with it: a provider
    /// that answers both in one message reads them after the intent
    /// committed, which keeps the order a separate read after the
    /// registration has (a removal that committed first is seen, one after
    /// meets the intent). The default registers and reads nothing.
    fn gc_register_mount(
        &self,
        intent: &GcIntent,
        _stack: LayerStackId,
        _node: NodeId,
    ) -> HistoryResult<MountMarkers> {
        self.gc_register(intent).map(|()| MountMarkers::NotRead)
    }

    /// Releases one admission intent, deleting its row.
    ///
    /// The default refuses; a provider that never registered one is unaffected.
    fn gc_release(&self, _op_id: &[u8; 16]) -> HistoryResult<()> {
        Err(crate::error::HistoryError::Unsupported(
            crate::error::NO_ADMISSION,
        ))
    }

    /// Marks one admission intent uncertain, so a later call reports
    /// `Unresolved` until its owner settles it. Returns `false` when no live
    /// or uncertain row has the op (none, or one already retained `done`).
    fn gc_mark_uncertain(&self, _op_id: &[u8; 16]) -> HistoryResult<bool> {
        Err(crate::error::HistoryError::Unsupported(
            crate::error::NO_ADMISSION,
        ))
    }

    /// Retains one released admission intent as the durable `done` audit row:
    /// its state becomes `done`, its owner columns are cleared so it cannot
    /// block a later mount, and `evidence` names the operator action, the
    /// operator and the operation. Returns `false` when no intent has
    /// `op_id` (already released or adopted), so a concurrent repair does not
    /// invent an audit row. The default refuses: a provider that keeps no
    /// `done` rows cannot promise the operator audit.
    ///
    /// # Errors
    ///
    /// The default returns [`crate::error::HistoryError::Unsupported`]; an
    /// implementation returns its own error when the row cannot be retained.
    fn gc_release_done(&self, _op_id: &[u8; 16], _evidence: &str) -> HistoryResult<bool> {
        Err(crate::error::HistoryError::Unsupported("GC done audit"))
    }

    /// Records `incarnation` as the current place of `machine` and settles
    /// the intents that value proves dead.
    ///
    /// One write transaction: the intents recorded at that machine are
    /// classified against the new ordered incarnation. Only a dead intent
    /// that is not uncertain is settled; an expired lease or an uncertain
    /// owner is kept, and another machine's rows are never read. The default
    /// refuses.
    fn gc_reconcile_machine(
        &self,
        _machine: &str,
        _incarnation: &str,
    ) -> HistoryResult<GcReconcile> {
        Err(crate::error::HistoryError::Unsupported(
            "GC place authority",
        ))
    }

    /// `branch.remove`: admits at Branch scope and writes the marker in one
    /// write transaction.
    fn gc_remove_branch(
        &self,
        branch: BranchId,
        removed_ms: i64,
        _intent: &GcIntent,
    ) -> HistoryResult<()> {
        // A provider without admission still writes the marker; the Store
        // service only adds admission where the provider keeps intents.
        self.mark_branch_removed(branch, removed_ms)
    }

    /// `project.delete`: admits at project scope and writes the marker in one
    /// write transaction, preserving the same-home retry classification.
    fn gc_delete_stack(
        &self,
        stack: LayerStackId,
        deleted_ms: i64,
        _intent: &GcIntent,
    ) -> HistoryResult<GcDeletion> {
        if let Some(deleted_ms) = self.stack_deleted_ms(stack)? {
            return Ok(GcDeletion::Retired { deleted_ms });
        }
        if self.layer_stack(stack)?.is_some() {
            self.mark_stack_deleted(stack, deleted_ms)?;
            return Ok(GcDeletion::Marked { deleted_ms });
        }
        Ok(match self.stack_deleted_ms(stack)? {
            Some(deleted_ms) => GcDeletion::Retired { deleted_ms },
            None => GcDeletion::Absent,
        })
    }

    /// `project.hide_path`: admits at project scope and advances the marker
    /// in one write transaction.
    fn gc_remove_path(
        &self,
        request: &PathRemovalRequest,
        _intent: &GcIntent,
    ) -> HistoryResult<PathRemovalOutcome> {
        self.remove_path(request)
    }

    /// The live intents recorded at one scope, for `store.status` and tests.
    fn gc_intents(
        &self,
        _scope_kind: GcScopeKind,
        _scope_id: &[u8],
    ) -> HistoryResult<Vec<GcIntent>> {
        Err(crate::error::HistoryError::Unsupported(
            crate::error::NO_ADMISSION,
        ))
    }

    /// What admission would say of each of `intents` now, in order: the
    /// owner evidence first, then the place view the caller installed for
    /// this thread, exactly as an admitting removal judges a conflicting
    /// intent. A read; it changes nothing. The operator repair's `list` and
    /// `preview` report it and its `settle` refuses a holder still `Live`
    /// unless forced. The default refuses.
    fn gc_liveness(&self, _intents: &[GcIntent]) -> HistoryResult<Vec<crate::Liveness>> {
        Err(crate::error::HistoryError::Unsupported(
            crate::error::NO_ADMISSION,
        ))
    }

    /// One bounded page of retained `done` admission rows, oldest first, for
    /// the operator audit. `page` is zero-based and `limit` is clamped to
    /// [`crate::records::MAXIMUM_AUDIT_PAGE`]. The live admission queries
    /// filter `state <> 'done'`; this read is the only one that sees them.
    fn gc_done_intents(&self, _page: u32, _limit: u32) -> HistoryResult<GcAuditPage> {
        Err(crate::error::HistoryError::Unsupported("GC audit"))
    }

    /// One bounded keyset page of the retained `done` rows of one Branch,
    /// oldest first by `(updated_ms, op_id)`: the rows strictly after
    /// `after`, or from the first. The operator `audit` action is scoped to
    /// the Branch it names.
    ///
    /// The default refuses: a provider that keeps no `done` rows has none.
    fn gc_done_intents_branch(
        &self,
        _project: &str,
        _branch: &str,
        _after: Option<GcAuditAfter>,
        _limit: u32,
    ) -> HistoryResult<GcBranchAuditPage> {
        Err(crate::error::HistoryError::Unsupported("GC done audit"))
    }
}

impl std::fmt::Debug for HistoryCatalogConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryCatalogConfig")
            .field("binding_key", &self.binding_key)
            .field("incarnation", &self.incarnation)
            .field("cursor_key", &"[redacted]")
            .finish()
    }
}

/// A held publication lane, released when dropped.
pub struct PublishLane(Option<Box<dyn std::any::Any>>);

impl PublishLane {
    /// Wraps a provider's hold; dropping the lane drops it.
    pub fn new(hold: Box<dyn std::any::Any>) -> Self {
        Self(Some(hold))
    }
}

impl std::fmt::Debug for PublishLane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PublishLane")
            .field(&self.0.is_some())
            .finish()
    }
}
