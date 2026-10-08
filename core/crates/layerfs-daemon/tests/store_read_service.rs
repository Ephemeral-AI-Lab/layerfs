//! Real Disposable Store reader admission, fairness and failure ownership.
#[path = "support/installed_store.rs"]
mod support;
use layerfs_content::{AuthenticatedObjects, ContentError, ObjectId};
use layerfs_daemon::{
    store::{
        BindRequest, PortError, ReadAdmissionError, ReadLease, ReadLimits, ReadTicket, Store,
        StoreReader,
    },
    Command, Owner, OwnerConfig,
};
use layerfs_history::{HistoryCatalog, HistoryError, WorkspaceId};
use layerfs_persistence::{Handles, StorageProvider};
use layerfs_storage::{
    location::PackInfo,
    port::{PackPersistence, PackReadChoice, PackReadPlan, PersistenceError},
    ReservationBlocks, Storage,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc,
    },
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};
const WAIT: Duration = Duration::from_secs(3);

struct Fixture {
    store: Arc<Store>,
    writer: Writer,
    readers: Vec<Arc<StorageProvider>>,
    root: ObjectId,
    input: Cleanup,
}
struct Writer {
    storage: Arc<StorageProvider>,
    history: Arc<layerfs_persistence::HistoryProvider>,
}
impl Fixture {
    fn new(label: &str, count: usize, limits: ReadLimits) -> Self {
        let input = support::Fixture::new(1, label);
        assert_eq!(input.count, 1);
        let opened =
            Handles::open_writable(input.config.clone(), support::BINDING, support::CURSOR)
                .unwrap();
        let writer = Writer {
            storage: opened.storage,
            history: Arc::new(opened.history),
        };
        let root = writer
            .history
            .branch_snapshot(input.branch)
            .unwrap()
            .unwrap()
            .effective_root;
        let mut readers = Vec::new();
        let sessions = (0..count)
            .map(|_| {
                let read = Handles::open_read_only(
                    input.config.clone(),
                    support::BINDING,
                    support::CURSOR,
                )
                .unwrap();
                readers.push(read.storage.clone());
                StoreReader::new(Storage::new(read.storage).unwrap(), Arc::new(read.history))
            })
            .collect();
        // Open an independently owned writer view of History, over the same
        // original writer session, without a second provider implementation.
        let history = writer.history.clone();
        let store = Arc::new(
            Store::new(
                writer.storage.clone(),
                history,
                sessions,
                0,
                ReservationBlocks::default(),
                limits,
            )
            .unwrap(),
        );
        Self {
            store,
            writer,
            readers,
            root,
            input: Cleanup(Some(input)),
        }
    }
    fn ticket(&self, id: u8) -> ReadTicket {
        self.store.read_ticket(Some(workspace(id))).unwrap()
    }
    fn branch(&self) -> layerfs_history::BranchId {
        self.input.0.as_ref().unwrap().branch
    }
}
struct Cleanup(Option<support::Fixture>);
impl Drop for Cleanup {
    fn drop(&mut self) {
        // Declared last, so original opened providers close before removal.
        self.0.take().unwrap().cleanup();
    }
}
fn workspace(id: u8) -> WorkspaceId {
    WorkspaceId::from_authority([id; 32]).unwrap()
}
fn limits() -> ReadLimits {
    ReadLimits {
        namespaces: 3,
        requests_per_namespace: 4,
    }
}
struct Event {
    sender: mpsc::SyncSender<()>,
    wakes: AtomicUsize,
    store: Arc<Store>,
}
impl Wake for Event {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        // Reentry verifies that user notification is outside the pool lock.
        self.store.read_work();
        self.wakes.fetch_add(1, Ordering::SeqCst);
        let _ = self.sender.try_send(());
    }
}
fn event(store: &Arc<Store>) -> (Arc<Event>, mpsc::Receiver<()>) {
    let (sender, receiver) = mpsc::sync_channel(1);
    (
        Arc::new(Event {
            sender,
            wakes: AtomicUsize::new(0),
            store: store.clone(),
        }),
        receiver,
    )
}
fn poll(
    ticket: &mut ReadTicket,
    event: &Arc<Event>,
) -> Poll<Result<ReadLease, ReadAdmissionError>> {
    let waker = Waker::from(event.clone());
    Pin::new(ticket).poll(&mut Context::from_waker(&waker))
}
fn lease(mut ticket: ReadTicket, store: &Arc<Store>) -> ReadLease {
    let (event, receiver) = event(store);
    let deadline = Instant::now() + WAIT;
    loop {
        match poll(&mut ticket, &event) {
            Poll::Ready(result) => return result.unwrap(),
            Poll::Pending => receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("original read ticket did not wake"),
        }
    }
}
fn observed(store: &Store, check: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !check() {
        assert!(
            Instant::now() < deadline,
            "read service observation: {:?}",
            store.read_work()
        );
        std::thread::yield_now();
    }
}

#[test]
fn workspace_rotation_and_fifo_prevent_a_hot_lane_owning_the_next_reader() {
    let f = Fixture::new("read-fairness", 1, limits());
    let held = lease(f.ticket(1), &f.store);
    let mut a2 = f.ticket(1);
    let mut a3 = f.ticket(1);
    let mut b1 = f.ticket(2);
    let (event, receiver) = event(&f.store);
    assert!(poll(&mut a2, &event).is_pending());
    assert!(poll(&mut a3, &event).is_pending());
    assert!(poll(&mut b1, &event).is_pending());
    assert_eq!(f.store.workspace_reads(workspace(1)).unwrap(), 3);
    drop(held);
    receiver.recv_timeout(WAIT).unwrap();
    let b = match poll(&mut b1, &event) {
        Poll::Ready(Ok(lease)) => lease,
        _ => panic!("other Workspace did not get next turn"),
    };
    assert!(poll(&mut a2, &event).is_pending());
    drop(b);
    receiver.recv_timeout(WAIT).unwrap();
    let a = match poll(&mut a2, &event) {
        Poll::Ready(Ok(lease)) => lease,
        _ => panic!("FIFO head not granted"),
    };
    assert!(poll(&mut a3, &event).is_pending());
    drop(a);
    drop(lease(a3, &f.store));
    assert_eq!(f.store.read_work().outstanding, 0);
    assert_eq!(f.store.read_work().grants, 4);
    assert_eq!(f.store.work().object_batches, 0);
}

#[test]
fn idle_selection_and_replaced_wakeup_do_not_wait_on_a_busy_reader() {
    let f = Fixture::new("read-idle", 2, limits());
    let first = lease(f.ticket(1), &f.store);
    let second = lease(f.ticket(2), &f.store);
    assert_ne!(first.index(), second.index());
    let mut next = f.ticket(3);
    let (old, _) = event(&f.store);
    let weak = Arc::downgrade(&old);
    assert!(poll(&mut next, &old).is_pending());
    drop(old);
    let (last, receiver) = event(&f.store);
    assert!(poll(&mut next, &last).is_pending());
    assert!(weak.upgrade().is_none());
    let index = second.index();
    drop(second);
    receiver.recv_timeout(WAIT).unwrap();
    let mut selected = match poll(&mut next, &last) {
        Poll::Ready(Ok(lease)) => lease,
        _ => panic!("idle reader missing"),
    };
    assert_eq!(selected.index(), index);
    assert_eq!(
        ObjectId::for_bytes(&selected.objects(&[f.root]).unwrap()[0]),
        f.root
    );
    drop((selected, first));
    assert_eq!(f.store.read_work().outstanding, 0);
}

#[test]
fn cancellation_capacity_and_stop_dispose_only_unattempted_admission() {
    let f = Fixture::new(
        "read-cancel",
        1,
        ReadLimits {
            namespaces: 1,
            requests_per_namespace: 2,
        },
    );
    let assigned = f.ticket(1);
    let queued = f.ticket(1);
    assert!(matches!(
        f.store.read_ticket(Some(workspace(1))),
        Err(ReadAdmissionError::Capacity)
    ));
    assert!(matches!(
        f.store.read_ticket(Some(workspace(2))),
        Err(ReadAdmissionError::Capacity)
    ));
    drop(assigned);
    let mut running = lease(queued, &f.store);
    let cancelled = f.ticket(1);
    drop(cancelled);
    let mut stopped = f.ticket(1);
    let (event, receiver) = event(&f.store);
    assert!(poll(&mut stopped, &event).is_pending());
    f.store.stop_reads();
    receiver.recv_timeout(WAIT).unwrap();
    assert!(matches!(
        poll(&mut stopped, &event),
        Poll::Ready(Err(ReadAdmissionError::Stopped))
    ));
    assert!(matches!(
        f.store.read_ticket(Some(workspace(1))),
        Err(ReadAdmissionError::Stopped)
    ));
    assert_eq!(
        running
            .snapshot(f.branch())
            .unwrap()
            .unwrap()
            .effective_root,
        f.root
    );
    assert_eq!(f.store.read_work().leased, 1);
    drop(running);
    assert_eq!(f.store.read_work().outstanding, 0);
}

struct Uncertain;
impl PackReadPlan for Uncertain {
    fn select(&mut self, _: PackInfo, _: &[u8]) -> Result<PackReadChoice, PersistenceError> {
        Err(PersistenceError::Uncertain)
    }
}
fn quarantine(provider: &StorageProvider, root: ObjectId) {
    let mut rows = Vec::new();
    provider.locate(&[root], &mut rows).unwrap();
    assert!(matches!(
        provider.read_scoped_pack(rows[0].location.pack_id, &mut Uncertain),
        Err(PersistenceError::Uncertain)
    ));
}
#[test]
fn quarantined_session_keeps_original_failure_and_only_healthy_reader_is_reused() {
    let f = Fixture::new("read-health", 2, limits());
    quarantine(&f.readers[0], f.root);
    let mut bad = lease(f.ticket(1), &f.store);
    assert_eq!(bad.index(), 0);
    assert!(f.store.reader_storage_diagnostics(0).is_none());
    let original = bad.objects(&[f.root]).unwrap_err();
    assert!(matches!(original.as_ref(), PortError::Storage(error) if error.is_unknown_outcome()));
    let before = f.store.work();
    assert!(Arc::ptr_eq(&original, &bad.objects(&[f.root]).unwrap_err()));
    assert_eq!(f.store.work(), before);
    drop(bad);
    assert_eq!(f.store.read_work().quarantined, 1);
    assert!(f.store.reader_storage_diagnostics(0).is_some());
    let failures = f.store.reader_failures();
    assert_eq!(failures[0].0, 0);
    assert!(Arc::ptr_eq(&failures[0].1, &original));
    let mut good = lease(f.ticket(2), &f.store);
    assert_eq!(good.index(), 1);
    assert_eq!(
        ObjectId::for_bytes(&good.objects(&[f.root]).unwrap()[0]),
        f.root
    );
    drop(good);
    assert_eq!(lease(f.ticket(3), &f.store).index(), 1);
}
#[test]
fn diagnostic_reader_snapshots_leave_admission_and_provider_work_unchanged() {
    for (count, label) in [(1, "reader-diag-one"), (4, "reader-diag-four")] {
        let f = Fixture::new(label, count, limits());
        let before = f.store.read_work();
        let demands = f.store.work();
        let sql: Vec<_> = f
            .readers
            .iter()
            .map(|reader| reader.diagnostics().unwrap().statements)
            .collect();
        for index in 0..count {
            assert!(f.store.reader_storage_diagnostics(index).is_some());
        }
        assert!(f.store.reader_storage_diagnostics(count).is_none());
        assert_eq!(f.store.read_work(), before);
        assert_eq!(f.store.work(), demands);
        let ticket = f.ticket(9);
        assert_eq!(f.store.read_work().assigned, 1);
        assert!(
            f.store.reader_storage_diagnostics(0).is_some(),
            "assigned reader should remain visible"
        );
        let held = lease(ticket, &f.store);
        let during = f.store.read_work();
        assert!(
            f.store.reader_storage_diagnostics(held.index()).is_none(),
            "active lease must be unavailable"
        );
        for index in 1..count {
            assert!(f.store.reader_storage_diagnostics(index).is_some());
        }
        assert_eq!(f.store.read_work(), during);
        assert_eq!(f.store.work(), demands);
        assert_eq!(
            f.readers
                .iter()
                .map(|reader| reader.diagnostics().unwrap().statements)
                .collect::<Vec<_>>(),
            sql
        );
        drop(held);
        assert!(f.store.reader_storage_diagnostics(0).is_some());
        assert_eq!(f.store.read_work().scheduler_bytes, before.scheduler_bytes);
        assert_eq!(f.store.read_work().outstanding, 0);
    }
}

#[test]
fn last_quarantine_wakes_waiters_with_terminal_no_reader_disposition() {
    let f = Fixture::new("read-all-quarantined", 1, limits());
    let mut held = lease(f.ticket(1), &f.store);
    let mut waiting = f.ticket(2);
    let (event, receiver) = event(&f.store);
    assert!(poll(&mut waiting, &event).is_pending());
    quarantine(&f.readers[0], f.root);
    assert!(held.objects(&[f.root]).is_err());
    drop(held);
    receiver.recv_timeout(WAIT).unwrap();
    assert!(matches!(
        poll(&mut waiting, &event),
        Poll::Ready(Err(ReadAdmissionError::NoReaders))
    ));
    assert!(matches!(
        f.store.read_ticket(Some(workspace(3))),
        Err(ReadAdmissionError::NoReaders)
    ));
    assert_eq!(f.store.read_work().outstanding, 0);
}

#[test]
fn waiting_operation_has_no_failure_lock_and_its_failures_do_not_poison_other_requests() {
    let f = Fixture::new("read-scopes", 1, limits());
    let held = lease(f.ticket(1), &f.store);
    let scope = f
        .writer
        .history
        .branch_snapshot(f.branch())
        .unwrap()
        .unwrap()
        .scope;
    let ports = f.store.ports_for(scope, workspace(2));
    let original = ports.clone();
    let root = f.root;
    let thread = std::thread::spawn(move || original.read_canonical(root));
    observed(&f.store, || f.store.read_work().waiting == 1);
    assert!(ports.failure().unwrap().is_none());
    assert!(ports.read_canonical(root).is_err());
    assert!(ports.failure().unwrap().is_none());
    drop(held);
    assert_eq!(ObjectId::for_bytes(&thread.join().unwrap().unwrap()), root);
    let missing = ObjectId::for_bytes(b"missing read-service object");
    assert_eq!(
        ports.read_canonical(missing),
        Err(ContentError::MissingObject)
    );
    let failure = ports.failure().unwrap().unwrap();
    let before = f.store.work();
    assert_eq!(ports.read_canonical(root), Err(ContentError::MissingObject));
    assert!(Arc::ptr_eq(&failure, &ports.failure().unwrap().unwrap()));
    assert_eq!(f.store.work(), before);
    let fresh = f.store.ports_for(scope, workspace(2));
    assert_eq!(
        ObjectId::for_bytes(&fresh.read_canonical(root).unwrap()),
        root
    );
    assert!(fresh.failure().unwrap().is_none());
    assert_eq!(f.store.read_work().quarantined, 0);
}

struct HeldPlan {
    entered: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
}
impl PackReadPlan for HeldPlan {
    fn select(&mut self, _: PackInfo, _: &[u8]) -> Result<PackReadChoice, PersistenceError> {
        self.entered.send(()).unwrap();
        self.release
            .recv_timeout(WAIT)
            .expect("held writer session deadline");
        Ok(PackReadChoice::Whole)
    }
}
#[test]
fn bind_reads_snapshot_while_the_original_writer_session_is_occupied() {
    let f = Fixture::new("read-bind", 1, limits());
    let mut rows = Vec::new();
    f.writer.storage.locate(&[f.root], &mut rows).unwrap();
    let id = rows[0].location.pack_id;
    let (entered, observed) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let provider = f.writer.storage.clone();
    let held = std::thread::spawn(move || {
        provider.read_scoped_pack(
            id,
            &mut HeldPlan {
                entered,
                release: released,
            },
        )
    });
    observed.recv_timeout(WAIT).unwrap();
    assert!(matches!(
        f.writer.history.branch_snapshot(f.branch()),
        Err(HistoryError::Busy)
    ));
    let owner = Owner::start(
        &f.input
            .0
            .as_ref()
            .unwrap()
            .directory
            .join("read-bind-overlay"),
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let bound = f
        .store
        .bind(
            owner.client(),
            BindRequest {
                branch: f.branch(),
                workspace: workspace(9),
            },
        )
        .unwrap();
    assert_eq!(bound.workspace.snapshot().unwrap().effective_root, f.root);
    release.send(()).unwrap();
    assert!(held.join().unwrap().is_ok());
    assert!(owner
        .client()
        .try_submit(Some(bound.workspace.route()), Command::Close)
        .unwrap()
        .wait()
        .unwrap()
        .result()
        .is_ok());
    drop(bound);
    owner.stop().unwrap();
}

#[test]
fn a_reader_grant_cannot_be_charged_to_a_different_workspace() {
    let f = Fixture::new("read-route", 1, limits());
    let scope = f
        .writer
        .history
        .branch_snapshot(f.branch())
        .unwrap()
        .unwrap()
        .scope;
    let mut reader = lease(f.ticket(1), &f.store);
    let other = f.store.ports_for(scope, workspace(2));
    let before = f.store.work();
    assert!(
        matches!(other.objects_on(&mut reader, &[f.root]), Err(error) if matches!(error.as_ref(), PortError::Storage(layerfs_storage::StorageError::Integrity("foreign Store/Workspace reader"))))
    );
    assert_eq!(f.store.work(), before);
    let original = f.store.ports_for(scope, workspace(1));
    assert_eq!(
        ObjectId::for_bytes(&original.objects_on(&mut reader, &[f.root]).unwrap()[0]),
        f.root
    );
    assert!(original.failure().unwrap().is_none());
}

#[test]
fn original_reader_release_races_future_registration_without_polling() {
    let f = Fixture::new("read-wake-race", 1, limits());
    for _ in 0..32 {
        let held = lease(f.ticket(1), &f.store);
        let pending = f.ticket(2);
        let release = std::thread::spawn(move || drop(held));
        let reader = lease(pending, &f.store);
        release.join().unwrap();
        assert_eq!(reader.workspace(), Some(workspace(2)));
        drop(reader);
    }
    assert_eq!(f.store.read_work().outstanding, 0);
    assert_eq!(f.store.read_work().grants, 64);
}
