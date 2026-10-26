//! shared backend history catalog contract delegation.
use super::{
    allocation, ancestry, branch, commit, hints, layerstack, merge, provider::HistoryProvider,
};
use layerfs_history::*;
impl HistoryCatalog for HistoryProvider {
    fn statistics(&self) -> HistoryResult<StoreStatistics> {
        HistoryProvider::statistics(self)
    }
    fn record_init(&self, totals: &InitStatistics) -> HistoryResult<()> {
        HistoryProvider::record_init(self, totals)
    }
    fn record_bindings(&self, rows: &[BindingHint]) -> HistoryResult<()> {
        self.write(|tx| hints::record(tx, rows))
    }
    fn bindings_of(&self, child: u64) -> HistoryResult<Vec<BindingHint>> {
        self.read(|tx| hints::of(tx, child))
    }
    fn record_file_digests(&self, rows: &[layerfs_history::FileDigest]) -> HistoryResult<()> {
        self.write(|tx| super::digests::record(tx, rows))
    }
    fn file_roots(&self, contents: &[([u8; 32], u64)]) -> HistoryResult<Vec<Option<[u8; 32]>>> {
        self.read(|tx| super::digests::roots(tx, contents))
    }
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

    fn layer_stack_named(&self, name: &HistoryName) -> HistoryResult<Option<LayerStackRecord>> {
        self.read(|tx| layerstack::layer_stack_named(tx, name))
    }

    fn branch(&self, id: BranchId) -> HistoryResult<Option<BranchRecord>> {
        self.read(|tx| branch::branch(tx, id))
    }

    fn branch_named(
        &self,
        stack: LayerStackId,
        name: &HistoryName,
    ) -> HistoryResult<Option<BranchRecord>> {
        self.read(|tx| branch::branch_named(tx, stack, name))
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

    fn overwritten(
        &self,
        layer: LayerId,
        position: u64,
        limit: u16,
    ) -> HistoryResult<Vec<LayerId>> {
        if limit == 0 || limit > layerfs_history::records::MAXIMUM_PAGE_RECORDS {
            return Err(layerfs_history::HistoryError::InvalidInput("page limit"));
        }
        self.read(|tx| super::lineage::overwritten(tx, layer, position, usize::from(limit)))
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

    fn merge_base(&self, left: NodeId, right: NodeId) -> HistoryResult<Option<NodeId>> {
        self.read(|tx| ancestry::merge_base(tx, left, right))
    }

    fn is_ancestor(&self, ancestor: NodeId, descendant: NodeId) -> HistoryResult<bool> {
        self.read(|tx| ancestry::is_ancestor(tx, ancestor, descendant))
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

    fn publish_lane(&self) -> HistoryResult<PublishLane> {
        let hold = self.session.hold_lane()?;
        Ok(PublishLane::new(Box::new(hold)))
    }

    fn commit_branch(&self, request: &CommitRequest) -> HistoryResult<CommitOutcome> {
        self.write(|tx| commit::commit_branch(tx, request))
    }

    fn add_layer(&self, request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome> {
        self.write(|tx| layerstack::add_layer(tx, request))
    }

    fn merge_branch(&self, request: &BranchMergeRequest) -> HistoryResult<BranchSnapshot> {
        self.write(|tx| merge::merge_branch(tx, request))
    }

    fn reserve_inodes(&self, request: &ReserveRequest) -> HistoryResult<Reservation> {
        self.write(|tx| allocation::reserve_inodes(tx, self.catalog_id, request))
    }
}
