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
        branch_snapshot(id: BranchId) -> HistoryResult<Option<BranchSnapshot>>;
        branches(stack: LayerStackId, page: &Page) -> HistoryResult<PageResult<BranchRecord>>;
        commit(id: CommitId) -> HistoryResult<Option<CommitRecord>>;
        layer(id: LayerId) -> HistoryResult<Option<LayerRecord>>;
        stage(workspace: WorkspaceId) -> HistoryResult<Option<StageRecord>>;
        stages(branch: BranchId, page: &Page) -> HistoryResult<PageResult<StageRecord>>;
        commit_history(request: &CommitHistoryRequest) -> HistoryResult<PageResult<CommitRecord>>;
        layer_history(request: &LayerHistoryRequest) -> HistoryResult<PageResult<LayerRecord>>;
        initialize_layerstack(request: &StackInitialization) -> HistoryResult<LayerStackRecord>;
        fork(request: &ForkRequest) -> HistoryResult<BranchSnapshot>;
        stage_changes(request: &StageRequest) -> HistoryResult<StageRecord>;
        commit_staged(request: &CommitStagedRequest) -> HistoryResult<CommitStagedOutcome>;
        add_layer(request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome>;
        discard_stage(request: &DiscardRequest) -> HistoryResult<DiscardOutcome>;
        reserve_inodes(request: &ReserveRequest) -> HistoryResult<Reservation>;
    }
    fn stage_and_commit(&self, request: &StageRequest) -> HistoryResult<CommitStagedOutcome> {
        if !self.once.swap(false, Ordering::SeqCst) {
            return self.handles.history.stage_and_commit(request);
        }
        match self.boundary {
            Boundary::HeldWriter => {
                let held = super::held_writer::HeldWriter::acquire(&self.database);
                let before = self.handles.diagnostics().unwrap();
                let result = self.handles.history.stage_and_commit(request);
                let after = self.handles.diagnostics().unwrap();
                assert_eq!(result, Err(HistoryError::Busy));
                assert_eq!(after.statements - before.statements, 1);
                assert_eq!(after.write_transactions, before.write_transactions);
                held.release();
                result
            }
            Boundary::LostAcknowledgement => {
                self.handles.history.stage_and_commit(request).unwrap();
                // Public-port acknowledgement loss, explicitly not a SQLite
                // I/O/quarantine qualification. Original caller remains unknown.
                Err(HistoryError::UnknownOutcome)
            }
        }
    }
}
