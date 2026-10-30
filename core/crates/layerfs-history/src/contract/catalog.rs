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
    CommitHistoryRequest, CommitOutcome, CommitRecord, CommitRequest, ForkRequest,
    LayerHistoryRequest, LayerRecord, LayerStackRecord, Page, PageResult, Reservation,
    ReserveRequest, StackInitialization,
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

/// Every semantic history operation, portable over its provider.
pub trait HistoryCatalog: Send + Sync {
    /// Identity this catalog was created or opened with.
    fn catalog_id(&self) -> CatalogId;

    /// Incarnation recorded when this catalog was created.
    fn incarnation(&self) -> u64;

    /// One stack by identity.
    fn layer_stack(&self, id: LayerStackId) -> HistoryResult<Option<LayerStackRecord>>;

    /// Stacks in name order.
    fn layer_stacks(&self, page: &Page) -> HistoryResult<PageResult<LayerStackRecord>>;

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

    /// One Branch by its name in a stack, through the name index: the
    /// stack's other Branches are not read.
    fn branch_named(
        &self,
        stack: LayerStackId,
        name: &HistoryName,
    ) -> HistoryResult<Option<BranchRecord>>;

    /// One coherent Branch snapshot with its resolved roots.
    fn branch_snapshot(&self, id: BranchId) -> HistoryResult<Option<BranchSnapshot>>;

    /// Branches of one stack in name order.
    fn branches(&self, stack: LayerStackId, page: &Page)
        -> HistoryResult<PageResult<BranchRecord>>;

    /// One Commit by identity.
    fn commit(&self, id: CommitId) -> HistoryResult<Option<CommitRecord>>;

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

    /// Publishes one saved candidate as a Commit against the expected Branch
    /// head in one write transaction. A moved head is `HeadMoved` unless the
    /// request forces an overwrite. UpToDate compares the current root.
    /// Unknown retains the original request; nothing is reread or replayed.
    fn commit_branch(&self, request: &CommitRequest) -> HistoryResult<CommitOutcome>;

    /// Publishes one Commit root as a new Layer against the expected stack head.
    fn add_layer(&self, request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome>;

    /// Creates a Branch from a merge of two sides, atomically.
    fn merge_branch(&self, request: &BranchMergeRequest) -> HistoryResult<BranchSnapshot>;

    /// Consumes one checked half-open inode range for a scope.
    fn reserve_inodes(&self, request: &ReserveRequest) -> HistoryResult<Reservation>;
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
