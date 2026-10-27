//! External orchestration at a real public history provider, never product hooks.
use layerfs_history::*;
use layerfs_persistence::Handles;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub enum Boundary {
    HeldWriter,
    LostAcknowledgement,
}
pub struct ObservedHistory {
    pub handles: Arc<Handles>,
    pub database: PathBuf,
    pub boundary: Boundary,
    pub once: AtomicBool,
}
macro_rules! forward {
    ($($name:ident($($arg:ident: $ty:ty),*) -> $out:ty;)*) => {$(
        fn $name(&self, $($arg: $ty),*) -> $out { self.handles.history.$name($($arg),*) }
    )*};
}
impl HistoryCatalog for ObservedHistory {
    forward! {
        catalog_id() -> CatalogId;
        incarnation() -> u64;
        layer_stack(id: LayerStackId) -> HistoryResult<Option<LayerStackRecord>>;
        layer_stacks(page: &Page) -> HistoryResult<PageResult<LayerStackRecord>>;
        branch(id: BranchId) -> HistoryResult<Option<BranchRecord>>;
        branch_named(stack: LayerStackId, name: &HistoryName) -> HistoryResult<Option<BranchRecord>>;
        branch_snapshot(id: BranchId) -> HistoryResult<Option<BranchSnapshot>>;
        branches(stack: LayerStackId, page: &Page) -> HistoryResult<PageResult<BranchRecord>>;
        commit(id: CommitId) -> HistoryResult<Option<CommitRecord>>;
        layer(id: LayerId) -> HistoryResult<Option<LayerRecord>>;
        commit_history(request: &CommitHistoryRequest) -> HistoryResult<PageResult<CommitRecord>>;
        layer_history(request: &LayerHistoryRequest) -> HistoryResult<PageResult<LayerRecord>>;
        initialize_layerstack(request: &StackInitialization) -> HistoryResult<LayerStackRecord>;
        fork(request: &ForkRequest) -> HistoryResult<BranchSnapshot>;
        merge_base(left: NodeId, right: NodeId) -> HistoryResult<Option<NodeId>>;
        is_ancestor(ancestor: NodeId, descendant: NodeId) -> HistoryResult<bool>;
        add_layer(request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome>;
        merge_branch(request: &BranchMergeRequest) -> HistoryResult<BranchSnapshot>;
        reserve_inodes(request: &ReserveRequest) -> HistoryResult<Reservation>;
    }
    fn commit_branch(&self, request: &CommitRequest) -> HistoryResult<CommitOutcome> {
        if !self.once.swap(false, Ordering::SeqCst) {
            return self.handles.history.commit_branch(request);
        }
        match self.boundary {
            Boundary::HeldWriter => {
                let held = super::held_writer::HeldWriter::acquire(&self.database);
                let before = self.handles.diagnostics().unwrap();
                let result = self.handles.history.commit_branch(request);
                let after = self.handles.diagnostics().unwrap();
                assert_eq!(result, Err(HistoryError::Busy));
                assert_eq!(after.statements - before.statements, 1);
                assert_eq!(after.write_transactions, before.write_transactions);
                held.release();
                result
            }
            Boundary::LostAcknowledgement => {
                self.handles.history.commit_branch(request).unwrap();
                // Public-port acknowledgement loss, explicitly not a SQLite
                // I/O/quarantine qualification. Original caller remains unknown.
                Err(HistoryError::UnknownOutcome)
            }
        }
    }
}
