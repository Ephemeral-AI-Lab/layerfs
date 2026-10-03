//! shared backend history catalog contract delegation.
use super::{allocation, branch, commit, layerstack, provider::HistoryProvider, staging};
use layerfs_history::*;
impl HistoryCatalog for HistoryProvider {
    fn catalog_id(&self) -> CatalogId {
        self.catalog_id
    }

    fn incarnation(&self) -> u64 {
        self.incarnation
    }

    fn layer_stack(&self, id: LayerStackId) -> HistoryResult<Option<LayerStackRecord>> {
        self.read(|tx| layerstack::layer_stack(tx, id))
    }

    fn layer_stacks(&self, page: &Page) -> HistoryResult<PageResult<LayerStackRecord>> {
        self.read(|tx| {
            layerstack::layer_stacks(
                tx,
                self.catalog_id,
                self.incarnation,
                &self.cursor_key,
                page,
            )
        })
    }

    fn branch(&self, id: BranchId) -> HistoryResult<Option<BranchRecord>> {
        self.read(|tx| branch::branch(tx, id))
    }

    fn branch_snapshot(&self, id: BranchId) -> HistoryResult<Option<BranchSnapshot>> {
        self.read(|tx| branch::branch_snapshot(tx, id))
    }

    fn branches(
        &self,
        stack: LayerStackId,
        page: &Page,
    ) -> HistoryResult<PageResult<BranchRecord>> {
        self.read(|tx| {
            branch::branches(
                tx,
                self.catalog_id,
                self.incarnation,
                &self.cursor_key,
                stack,
                page,
            )
        })
    }

    fn commit(&self, id: CommitId) -> HistoryResult<Option<CommitRecord>> {
        self.read(|tx| commit::commit(tx, id))
    }

    fn layer(&self, id: LayerId) -> HistoryResult<Option<LayerRecord>> {
        self.read(|tx| layerstack::layer(tx, id))
    }

    fn stage(&self, workspace: WorkspaceId) -> HistoryResult<Option<StageRecord>> {
        self.read(|tx| staging::stage(tx, workspace))
    }

    fn stages(&self, branch: BranchId, page: &Page) -> HistoryResult<PageResult<StageRecord>> {
        self.read(|tx| {
            staging::stages(
                tx,
                self.catalog_id,
                self.incarnation,
                &self.cursor_key,
                branch,
                page,
            )
        })
    }

    fn commit_history(
        &self,
        request: &CommitHistoryRequest,
    ) -> HistoryResult<PageResult<CommitRecord>> {
        self.read(|tx| {
            commit::commit_history(
                tx,
                self.catalog_id,
                self.incarnation,
                &self.cursor_key,
                request,
            )
        })
    }

    fn layer_history(
        &self,
        request: &LayerHistoryRequest,
    ) -> HistoryResult<PageResult<LayerRecord>> {
        self.read(|tx| {
            layerstack::layer_history(
                tx,
                self.catalog_id,
                self.incarnation,
                &self.cursor_key,
                request,
            )
        })
    }

    fn initialize_layerstack(
        &self,
        request: &StackInitialization,
    ) -> HistoryResult<LayerStackRecord> {
        self.write(|tx| layerstack::initialize_layerstack(tx, request))
    }

    fn fork(&self, request: &ForkRequest) -> HistoryResult<BranchSnapshot> {
        self.write(|tx| branch::fork(tx, request))
    }

    fn stage_changes(&self, request: &StageRequest) -> HistoryResult<StageRecord> {
        self.write(|tx| staging::stage_changes(tx, request))
    }

    fn commit_staged(&self, request: &CommitStagedRequest) -> HistoryResult<CommitStagedOutcome> {
        let mut observed = None;
        self.write(|tx| commit::commit_staged(tx, request, &mut observed))
            .map_err(|error| error.with_observed_stage(request.workspace, observed))
    }

    fn add_layer(&self, request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome> {
        self.write(|tx| layerstack::add_layer(tx, request))
    }

    fn discard_stage(&self, request: &DiscardRequest) -> HistoryResult<DiscardOutcome> {
        let mut observed = None;
        self.write(|tx| staging::discard_stage(tx, request, &mut observed))
            .map_err(|error| error.with_observed_stage(request.workspace, observed))
    }

    fn reserve_inodes(&self, request: &ReserveRequest) -> HistoryResult<Reservation> {
        self.write(|tx| allocation::reserve_inodes(tx, self.catalog_id, request))
    }
}
