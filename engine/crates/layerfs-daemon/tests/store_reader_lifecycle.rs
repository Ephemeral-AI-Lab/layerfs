//! Real read-only connections: mount lifetime, stale clients, failed bind and stop races.
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod support;
use layerfs_content::AuthenticatedObjects;
use layerfs_daemon::{
    bootstrap::open_store_observed,
    store::{BindError, BindRequest, PortError, ReaderFactory, Store, StoreReader},
    Owner, OwnerConfig,
};
use layerfs_history::*;
use layerfs_persistence::{Handles, StorageProvider};
use layerfs_storage::{ReservationBlocks, Storage, StorageError};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc, Mutex, Weak,
    },
    time::Duration,
};
fn request(f: &support::Fixture, tag: u8) -> BindRequest {
    BindRequest {
        branch: f.branch,
        workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
    }
}
fn owner(f: &support::Fixture) -> Owner {
    Owner::start(
        &f.directory.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig { namespaces: 16 },
    )
    .unwrap()
}
#[test]
fn fresh_mounts_close_connections_and_old_clients_never_borrow_a_replacement() {
    let f = support::Fixture::new(2, "reader-remount");
    let opened = open_store_observed(
        f.config.clone(),
        support::BINDING,
        support::CURSOR,
        2,
        0,
        ReservationBlocks::default(),
    )
    .unwrap();
    let owner = owner(&f);
    assert_eq!(opened.store.read_handles(), 1);
    let mut old = Vec::new();
    for cycle in 0..4 {
        let bound = opened
            .store
            .bind(owner.client(), request(&f, 81 + cycle))
            .unwrap()
            .workspace;
        let snapshot = bound.snapshot().unwrap();
        let operation = bound.operation().unwrap();
        let ports = operation.ports().fresh();
        ports
            .read_canonical_batch(&[snapshot.effective_root])
            .unwrap();
        assert_eq!(opened.store.read_handles(), 2);
        bound.close().unwrap();
        assert_eq!(opened.store.read_handles(), 1);
        // Retained request and diagnostic references cannot keep a healthy reader open.
        assert_eq!(opened.diagnostics().unwrap().readers.len(), 2); // control + fixed retired sum
        old.push((operation, ports, snapshot.effective_root));
        for (operation, ports, root) in &old {
            assert!(ports.fresh().read_canonical_batch(&[*root]).is_err());
            assert!(operation
                .ports()
                .fresh()
                .read_canonical_batch(&[*root])
                .is_err());
        }
    }
    // One Owner never reuses an incarnation. A replacement Owner over the
    // still-open Store may bind the same public ID; old ports must stay closed.
    owner.stop().unwrap();
    let directory = f.directory.join("replacement-owner");
    std::fs::create_dir(&directory).unwrap();
    let replacement = Owner::start(
        &directory.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let bound = opened
        .store
        .bind(replacement.client(), request(&f, 81))
        .unwrap()
        .workspace;
    let before = opened.store.read_work().requests;
    for (_, ports, root) in &old {
        assert!(ports.fresh().read_canonical_batch(&[*root]).is_err());
    }
    assert_eq!(opened.store.read_work().requests, before);
    bound.close().unwrap();
    opened.store.stop_reads();
    opened.store.stop_reads();
    assert_eq!(opened.store.read_handles(), 0);
    assert!(opened.store.read_work().stopping);
    drop((old, bound, opened));
    replacement.stop().unwrap();
    f.cleanup();
}
#[test]
fn a_failed_bind_closes_its_healthy_reader_and_returns_the_original_request() {
    let f = support::Fixture::new(1, "reader-failed-bind");
    let opened = open_store_observed(
        f.config.clone(),
        support::BINDING,
        support::CURSOR,
        1,
        0,
        ReservationBlocks::default(),
    )
    .unwrap();
    let owner = owner(&f);
    let mut input = request(&f, 82);
    input.branch = BranchId::from_authority([99; 16]);
    let failure = opened
        .store
        .bind(owner.client(), input.clone())
        .err()
        .unwrap();
    assert_eq!(failure.request.branch, input.branch);
    assert_eq!(failure.request.workspace, input.workspace);
    assert!(matches!(failure.error, BindError::MissingBranch(branch) if branch == input.branch));
    assert_eq!(opened.store.read_handles(), 1);
    assert_eq!(opened.diagnostics().unwrap().readers.len(), 2);
    let bound = opened
        .store
        .bind(owner.client(), request(&f, 82))
        .unwrap()
        .workspace;
    bound.close().unwrap();
    drop((failure, bound, opened));
    owner.stop().unwrap();
    f.cleanup();
}
#[derive(Clone, Copy)]
enum Fault {
    Unknown,
    Panic,
}
struct FaultHistory {
    handles: Arc<Handles>,
    fault: Option<Fault>,
    calls: Arc<AtomicUsize>,
}
macro_rules! forward {
    ($($name:ident($($arg:ident: $ty:ty),*) -> $out:ty;)*) => {$(
        fn $name(&self, $($arg: $ty),*) -> $out { self.handles.history.$name($($arg),*) }
    )*};
}
impl HistoryCatalog for FaultHistory {
    forward! {
        catalog_id() -> CatalogId;
        incarnation() -> u64;
        layer_stack(id: LayerStackId) -> HistoryResult<Option<LayerStackRecord>>;
        layer_stacks(page: &Page) -> HistoryResult<PageResult<LayerStackRecord>>;
        branch(id: BranchId) -> HistoryResult<Option<BranchRecord>>;
        branch_named(stack: LayerStackId, name: &HistoryName) -> HistoryResult<Option<BranchRecord>>;
        branches(stack: LayerStackId, page: &Page) -> HistoryResult<PageResult<BranchRecord>>;
        commit(id: CommitId) -> HistoryResult<Option<CommitRecord>>;
        layer(id: LayerId) -> HistoryResult<Option<LayerRecord>>;
        commit_history(request: &CommitHistoryRequest) -> HistoryResult<PageResult<CommitRecord>>;
        layer_history(request: &LayerHistoryRequest) -> HistoryResult<PageResult<LayerRecord>>;
        initialize_layerstack(request: &StackInitialization) -> HistoryResult<LayerStackRecord>;
        fork(request: &ForkRequest) -> HistoryResult<BranchSnapshot>;
        merge_base(left: NodeId, right: NodeId) -> HistoryResult<Option<NodeId>>;
        is_ancestor(ancestor: NodeId, descendant: NodeId) -> HistoryResult<bool>;
        commit_branch(request: &CommitRequest) -> HistoryResult<CommitOutcome>;
        add_layer(request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome>;
        merge_branch(request: &BranchMergeRequest) -> HistoryResult<BranchSnapshot>;
        reserve_inodes(request: &ReserveRequest) -> HistoryResult<Reservation>;
    }
    fn branch_snapshot(&self, branch: BranchId) -> HistoryResult<Option<BranchSnapshot>> {
        let Some(fault) = self.fault else {
            return self.handles.history.branch_snapshot(branch);
        };
        self.calls.fetch_add(1, Ordering::Relaxed);
        match fault {
            Fault::Unknown => Err(HistoryError::UnknownOutcome),
            Fault::Panic => panic!("original reader provider panic"),
        }
    }
}
type Connections = Arc<Mutex<Vec<Weak<StorageProvider>>>>;
fn fault_store(f: &support::Fixture, fault: Fault) -> (Arc<Store>, Arc<AtomicUsize>, Connections) {
    let writer =
        Handles::open_writable(f.config.clone(), support::BINDING, support::CURSOR).unwrap();
    let config = f.config.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let weak = Arc::new(Mutex::new(Vec::new()));
    let seen = weak.clone();
    let demands = calls.clone();
    let factory: ReaderFactory = Arc::new(move |index| {
        let read = Arc::new(Handles::open_read_only(
            config.clone(),
            support::BINDING,
            support::CURSOR,
        )?);
        seen.lock().unwrap().push(Arc::downgrade(&read.storage));
        let history = Arc::new(FaultHistory {
            handles: read.clone(),
            fault: (index == 1).then_some(fault),
            calls: demands.clone(),
        });
        Ok(StoreReader::new(
            Storage::new(read.storage.clone())?,
            history,
        ))
    });
    let control = factory(0).unwrap();
    let store = Store::new(
        writer.storage,
        Arc::new(writer.history),
        control,
        factory,
        0,
        ReservationBlocks::default(),
        2,
    )
    .unwrap();
    (Arc::new(store), calls, weak)
}
#[test]
fn unknown_snapshot_retains_the_exact_reader_cause_and_never_replays_it() {
    let f = support::Fixture::new(1, "reader-unknown");
    let (store, calls, weak) = fault_store(&f, Fault::Unknown);
    let owner = owner(&f);
    let input = request(&f, 83);
    let failure = store.bind(owner.client(), input.clone()).err().unwrap();
    let cause = match failure.error {
        BindError::Cleanup { original, cause } => {
            assert!(matches!(*original, BindError::Read(ref error) if Arc::ptr_eq(error, &cause)));
            cause
        }
        other => panic!("{other:?}"),
    };
    assert!(matches!(
        cause.as_ref(),
        PortError::History(HistoryError::UnknownOutcome)
    ));
    assert!(Arc::ptr_eq(&store.reader_failures()[0].1, &cause));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(store.bind(owner.client(), input).is_err());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    store.stop_reads();
    assert_eq!(store.read_handles(), 1);
    assert!(Arc::ptr_eq(&store.reader_failures()[0].1, &cause));
    assert!(weak.lock().unwrap()[0].upgrade().is_none());
    assert!(weak.lock().unwrap()[1].upgrade().is_some());
    drop((cause, store));
    owner.stop().unwrap();
    f.cleanup();
}
#[test]
fn a_poisoned_reader_is_retained_with_one_original_cause_at_shutdown() {
    let f = support::Fixture::new(1, "reader-poison");
    let (store, calls, weak) = fault_store(&f, Fault::Panic);
    let owner = owner(&f);
    let child_store = store.clone();
    let client = owner.client();
    let input = request(&f, 84);
    let child = std::thread::spawn(move || child_store.bind(client, input));
    assert!(child.join().is_err());
    let first = store.reader_failures()[0].1.clone();
    assert!(matches!(first.as_ref(), PortError::Poisoned));
    store.stop_reads();
    store.stop_reads();
    assert!(Arc::ptr_eq(&store.reader_failures()[0].1, &first));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(store.read_handles(), 1);
    assert!(weak.lock().unwrap()[1].upgrade().is_some());
    drop((first, store));
    owner.stop().unwrap();
    f.cleanup();
}
#[test]
fn shutdown_racing_a_bind_closes_the_acquired_reader_without_reopening() {
    let f = support::Fixture::new(1, "reader-stop-race");
    let writer =
        Handles::open_writable(f.config.clone(), support::BINDING, support::CURSOR).unwrap();
    let config = f.config.clone();
    let (entered, started) = mpsc::channel();
    let (release, parked) = mpsc::channel();
    let parked = Mutex::new(parked);
    let weak = Arc::new(Mutex::new(Vec::new()));
    let seen = weak.clone();
    let factory: ReaderFactory = Arc::new(move |index| {
        let read = Handles::open_read_only(config.clone(), support::BINDING, support::CURSOR)?;
        seen.lock().unwrap().push(Arc::downgrade(&read.storage));
        let reader = StoreReader::new(Storage::new(read.storage)?, Arc::new(read.history));
        if index > 0 {
            entered.send(()).unwrap();
            parked
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .map_err(|_| StorageError::Integrity("bounded reader-opening wait expired"))?;
        }
        Ok(reader)
    });
    let control = factory(0).unwrap();
    let store = Arc::new(
        Store::new(
            writer.storage,
            Arc::new(writer.history),
            control,
            factory,
            0,
            ReservationBlocks::default(),
            1,
        )
        .unwrap(),
    );
    let owner = owner(&f);
    let child_store = store.clone();
    let client = owner.client();
    let input = request(&f, 85);
    let child = std::thread::spawn(move || child_store.bind(client, input));
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    store.stop_reads();
    release.send(()).unwrap();
    let failure = child.join().unwrap().err().unwrap();
    assert!(
        matches!(failure.error, BindError::Read(ref error) if matches!(error.as_ref(), PortError::ReadAdmission(layerfs_daemon::store::ReadAdmissionError::Stopped)))
    );
    assert_eq!(store.read_handles(), 0);
    assert_eq!(weak.lock().unwrap().len(), 2);
    assert!(weak
        .lock()
        .unwrap()
        .iter()
        .all(|connection| connection.upgrade().is_none()));
    drop((failure, store));
    owner.stop().unwrap();
    f.cleanup();
}
