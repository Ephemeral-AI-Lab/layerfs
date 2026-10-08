//! External orchestration at a real public history provider, never product
//! hooks: the Commit thread's one publication is held on a chosen side of the
//! real `stage_and_commit` until the test releases it.
//!
//! Every wait is bounded. The held thread proceeds by itself when its limit
//! expires and records that it did; the test's `Hold` releases it when
//! dropped, on every path including a panic.
use layerfs_history::*;
use layerfs_persistence::Handles;
use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

/// Which side of the real publication the Commit thread is held on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Point {
    /// Before the call: nothing of this Commit is published while held.
    BeforePublication,
    /// After the call returned: its outcome is fixed and the driver has not
    /// yet started the local install.
    BeforeInstall,
}
#[derive(Default)]
struct State {
    /// The next publication is the held one; later ones pass straight through.
    armed: bool,
    entered: bool,
    released: bool,
    expired: bool,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    limit: Duration,
}
impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
    /// Blocks the calling Commit thread until release or its own limit.
    fn hold(&self) {
        let mut state = self.lock();
        state.entered = true;
        self.changed.notify_all();
        let deadline = Instant::now() + self.limit;
        while !state.released {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                state.expired = true;
                break;
            }
            state = self
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
    }
}
/// The real catalog with one gated publication.
pub struct GatedHistory {
    pub handles: Arc<Handles>,
    point: Point,
    shared: Arc<Shared>,
}
/// The test's side of the gate.
pub struct Hold(Arc<Shared>);
/// Wraps `handles`' catalog. `limit` bounds how long the Commit thread stays
/// held if nothing releases it.
pub fn gated(handles: Arc<Handles>, point: Point, limit: Duration) -> (GatedHistory, Hold) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            armed: true,
            ..State::default()
        }),
        changed: Condvar::new(),
        limit,
    });
    (
        GatedHistory {
            handles,
            point,
            shared: shared.clone(),
        },
        Hold(shared),
    )
}
impl Hold {
    /// Bounded observation: true once the Commit thread is held at the gate.
    pub fn entered(&self, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        let mut state = self.0.lock();
        while !state.entered {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            state = self
                .0
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
        true
    }
    /// Lets the held thread continue. Idempotent.
    pub fn release(&self) {
        self.0.lock().released = true;
        self.0.changed.notify_all();
    }
    /// True if the held thread left by its own limit, not by a release.
    pub fn expired(&self) -> bool {
        self.0.lock().expired
    }
}
impl Drop for Hold {
    fn drop(&mut self) {
        self.release();
    }
}
macro_rules! forward {
    ($($name:ident($($arg:ident: $ty:ty),*) -> $out:ty;)*) => {$(
        fn $name(&self, $($arg: $ty),*) -> $out { self.handles.history.$name($($arg),*) }
    )*};
}
impl HistoryCatalog for GatedHistory {
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
        let held = std::mem::take(&mut self.shared.lock().armed);
        if !held {
            return self.handles.history.stage_and_commit(request);
        }
        match self.point {
            Point::BeforePublication => {
                self.shared.hold();
                self.handles.history.stage_and_commit(request)
            }
            Point::BeforeInstall => {
                // The real, original outcome; only its return is delayed.
                let outcome = self.handles.history.stage_and_commit(request);
                self.shared.hold();
                outcome
            }
        }
    }
}
