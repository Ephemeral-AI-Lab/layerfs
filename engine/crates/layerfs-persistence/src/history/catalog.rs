//! shared backend history catalog contract delegation.
use super::{
    allocation, ancestry, branch, commit, hints, layerstack, merge, provider::HistoryProvider,
};
use layerfs_history::*;
impl HistoryCatalog for HistoryProvider {
    fn query_authenticator(&self, payload: &[u8]) -> HistoryResult<[u8; 32]> {
        layerfs_history::query::authenticate_query(
            &self.cursor_key,
            self.catalog_id,
            self.incarnation,
            payload,
        )
    }
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
        self.write(|tx| super::digests::record(tx, rows, self.digest_floor))
    }
    fn file_roots(&self, contents: &[([u8; 32], u64)]) -> HistoryResult<Vec<Option<[u8; 32]>>> {
        super::digests::roots(self, contents)
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

    // One statement (#96): a Sandbox create's reach check finds its project.
    fn layer_stack_named(&self, name: &HistoryName) -> HistoryResult<Option<LayerStackRecord>> {
        self.read_statement(|tx| layerstack::layer_stack_named(tx, name))
    }

    // The collision statement reads the retired/deleted holder on purpose.
    fn layer_stack_name_taken(
        &self,
        name: &HistoryName,
    ) -> HistoryResult<Option<layerfs_history::error::NameTakenState>> {
        self.read(|tx| layerstack::name_taken(tx, name))
    }

    fn branch(&self, id: BranchId) -> HistoryResult<Option<BranchRecord>> {
        self.read(|tx| branch::branch(tx, id))
    }

    // One statement, no visibility filter: rendering keeps a retired Branch.
    fn branch_any(&self, id: BranchId) -> HistoryResult<Option<BranchRecord>> {
        self.read_statement(|tx| branch::branch_any(tx, id))
    }

    fn branch_named(
        &self,
        stack: LayerStackId,
        name: &HistoryName,
    ) -> HistoryResult<Option<BranchRecord>> {
        self.read(|tx| branch::branch_named(tx, stack, name))
    }

    fn branch_snapshot(&self, id: BranchId) -> HistoryResult<Option<BranchSnapshot>> {
        self.read_statement(|tx| branch::branch_snapshot(tx, id))
    }

    fn branch_snapshot_named(
        &self,
        stack: &HistoryName,
        branch: &HistoryName,
    ) -> HistoryResult<NamedSnapshot> {
        self.read_statement(|tx| branch::branch_snapshot_named(tx, stack, branch))
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

    // One statement each (#95): a Commit's `logical.known` reads one row.
    fn commit(&self, id: CommitId) -> HistoryResult<Option<CommitRecord>> {
        self.read_statement(|tx| commit::commit(tx, id))
    }

    fn layer(&self, id: LayerId) -> HistoryResult<Option<LayerRecord>> {
        self.read_statement(|tx| layerstack::layer(tx, id))
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

    fn remove_layerstack(&self, stack: LayerStackId) -> HistoryResult<()> {
        self.write(|tx| super::removal::remove_layerstack(tx, stack))
    }

    fn mark_stack_deleted(&self, stack: LayerStackId, deleted_ms: i64) -> HistoryResult<()> {
        self.write(|tx| layerstack::mark_stack_deleted(tx, stack, deleted_ms))
    }

    fn stack_deleted_ms(&self, stack: LayerStackId) -> HistoryResult<Option<i64>> {
        self.read(|tx| layerstack::stack_deleted_ms(tx, stack))
    }

    fn publish_lane(&self) -> HistoryResult<PublishLane> {
        match &self.engine {
            crate::backend::Engine::Sqlite(session) => {
                Ok(PublishLane::new(Box::new(session.hold_lane()?)))
            }
            #[cfg(feature = "postgresql")]
            crate::backend::Engine::Postgres(session) => {
                Ok(PublishLane::new(Box::new(session.hold_lane()?)))
            }
        }
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

    fn mark_branch_removed(&self, branch: BranchId, removed_ms: i64) -> HistoryResult<()> {
        self.write(|tx| super::removal::mark_branch_removed(tx, branch, removed_ms))
    }

    fn reserve_inodes(&self, request: &ReserveRequest) -> HistoryResult<Reservation> {
        self.write(|tx| allocation::reserve_inodes(tx, self.catalog_id, request))
    }

    fn mark_path_removed(
        &self,
        stack: LayerStackId,
        path: &[u8],
        frontier_serial: u64,
        removed_ms: i64,
    ) -> HistoryResult<()> {
        self.write(|tx| super::paths::mark(tx, stack, path, frontier_serial, removed_ms))
    }

    fn remove_path(&self, request: &PathRemovalRequest) -> HistoryResult<PathRemovalOutcome> {
        self.write(|tx| super::paths::remove_path(tx, request))
    }

    fn path_removals(&self, stack: LayerStackId) -> HistoryResult<Vec<PathRemovalRecord>> {
        self.read(|tx| super::paths::markers(tx, stack))
    }

    fn node_history_serial(&self, node: NodeId) -> HistoryResult<Option<u64>> {
        self.read_statement(|tx| super::position::node_serial(tx, node))
    }

    fn gc_register(&self, intent: &GcIntent) -> HistoryResult<()> {
        super::admission::register(self, intent)
    }

    fn gc_release(&self, op_id: &[u8; 16]) -> HistoryResult<()> {
        super::admission::release(self, op_id)
    }

    fn gc_mark_uncertain(&self, op_id: &[u8; 16]) -> HistoryResult<()> {
        super::admission::mark_uncertain(self, op_id)
    }

    fn gc_remove_branch(
        &self,
        branch: BranchId,
        removed_ms: i64,
        intent: &GcIntent,
    ) -> HistoryResult<()> {
        super::admission::remove_branch(self, branch, removed_ms, intent)
    }

    fn gc_delete_stack(
        &self,
        stack: LayerStackId,
        deleted_ms: i64,
        intent: &GcIntent,
    ) -> HistoryResult<GcDeletion> {
        super::admission::delete_stack(self, stack, deleted_ms, intent)
    }

    fn gc_remove_path(
        &self,
        request: &PathRemovalRequest,
        intent: &GcIntent,
    ) -> HistoryResult<PathRemovalOutcome> {
        super::admission::remove_path(self, request, intent)
    }

    fn gc_intents(&self, scope_kind: GcScopeKind, scope_id: &[u8]) -> HistoryResult<Vec<GcIntent>> {
        super::admission::intents(self, scope_kind, scope_id)
    }
}
