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
use crate::identity::{BranchId, CatalogId, CommitId, LayerId, LayerStackId, WorkspaceId};
use crate::records::{
    AddLayerOutcome, AddLayerRequest, BranchRecord, BranchSnapshot, CommitHistoryRequest,
    CommitRecord, CommitStagedOutcome, CommitStagedRequest, DiscardOutcome, DiscardRequest,
    ForkRequest, LayerHistoryRequest, LayerRecord, LayerStackRecord, Page, PageResult, Reservation,
    ReserveRequest, StackInitialization, StageRecord, StageRequest,
};
/// Checked inputs that establish one catalog binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryCatalogConfig {
    /// Stable authority binding key. It identifies the content authority this
    /// catalog is bound to; the caller is responsible for having selected the
    /// right physical database for it.
    pub binding_key: Vec<u8>,
    /// Catalog incarnation. A fresh creation states it; a cursor binds it, so a
    /// cursor from a recreated catalog cannot be replayed into this one.
    pub incarnation: u64,
}

impl HistoryCatalogConfig {
    /// Checks both inputs.
    pub fn check(&self) -> HistoryResult<()> {
        CatalogId::derive(&self.binding_key)?;
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

    /// One Branch by identity.
    fn branch(&self, id: BranchId) -> HistoryResult<Option<BranchRecord>>;

    /// One coherent Branch snapshot with its resolved roots.
    fn branch_snapshot(&self, id: BranchId) -> HistoryResult<Option<BranchSnapshot>>;

    /// Branches of one stack in name order.
    fn branches(&self, stack: LayerStackId, page: &Page)
        -> HistoryResult<PageResult<BranchRecord>>;

    /// One Commit by identity.
    fn commit(&self, id: CommitId) -> HistoryResult<Option<CommitRecord>>;

    /// One Layer by identity.
    fn layer(&self, id: LayerId) -> HistoryResult<Option<LayerRecord>>;

    /// One stage by producer incarnation.
    fn stage(&self, workspace: WorkspaceId) -> HistoryResult<Option<StageRecord>>;

    /// Stages of one Branch in token order.
    fn stages(&self, branch: BranchId, page: &Page) -> HistoryResult<PageResult<StageRecord>>;

    /// Ancestry walk of one Branch, newest first, bounded by
    /// [`crate::records::MAXIMUM_LINEAGE_ROWS`].
    fn commit_history(
        &self,
        request: &CommitHistoryRequest,
    ) -> HistoryResult<PageResult<CommitRecord>>;

    /// Publication chain of one stack, newest first, bounded by
    /// [`crate::records::MAXIMUM_LINEAGE_ROWS`].
    fn layer_history(
        &self,
        request: &LayerHistoryRequest,
    ) -> HistoryResult<PageResult<LayerRecord>>;

    /// Creates the genesis Layer and the stack atomically.
    fn initialize_layerstack(
        &self,
        request: &StackInitialization,
    ) -> HistoryResult<LayerStackRecord>;

    /// Creates one Branch sharing the selected ancestry.
    fn fork(&self, request: &ForkRequest) -> HistoryResult<BranchSnapshot>;

    /// Inserts one frozen stage with its own freshly allocated token.
    fn stage_changes(&self, request: &StageRequest) -> HistoryResult<StageRecord>;

    /// Commits one exact stage against its frozen expectations.
    fn commit_staged(&self, request: &CommitStagedRequest) -> HistoryResult<CommitStagedOutcome>;

    /// Publishes one Commit root as a new Layer against the expected stack head.
    fn add_layer(&self, request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome>;

    /// Removes exactly one stage by token.
    fn discard_stage(&self, request: &DiscardRequest) -> HistoryResult<DiscardOutcome>;

    /// Consumes one checked half-open inode range for a scope.
    fn reserve_inodes(&self, request: &ReserveRequest) -> HistoryResult<Reservation>;
}
