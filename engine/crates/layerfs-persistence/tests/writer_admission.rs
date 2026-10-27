//! Cooperative Store admission precedes one public transaction attempt.
mod support;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{location::PackInfo, port::*, Storage, StoragePolicy};
#[cfg(target_os = "linux")]
use std::{
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    process::{Child, Command, Stdio},
    time::Instant,
};
use std::{path::Path, sync::mpsc, time::Duration};

fn authority() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"writer-admission".to_vec(),
        cursor_key: [71; 32],
        incarnation: 1,
    }
}
fn config(path: &Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}
fn create(path: &Path) -> Handles {
    Handles::create(config(path), StoragePolicy::frozen_default(), &authority()).unwrap()
}
#[cfg(target_os = "linux")]
fn open(path: &Path) -> Handles {
    Handles::open_writable(
        config(path),
        &authority().binding_key,
        authority().cursor_key,
    )
    .unwrap()
}
fn reserve() -> Reserve {
    Reserve {
        packs: 1,
        ordinals: 1,
    }
}

struct HeldPlan {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
impl PackReadPlan for HeldPlan {
    fn select(&mut self, _: PackInfo, _: &[u8]) -> Result<PackReadChoice, PersistenceError> {
        self.entered
            .send(())
            .map_err(|e| PersistenceError::Refused {
                status: e.to_string(),
            })?;
        self.release
            .recv_timeout(Duration::from_secs(10))
            .map_err(|e| PersistenceError::Refused {
                status: e.to_string(),
            })?;
        Ok(PackReadChoice::Whole)
    }
}

#[test]
fn shared_session_reservation_waits_for_an_ordinary_public_read_owner() {
    let temp = support::Temp::new("writer-admission-local");
    let handles = create(&temp.join("store.sqlite"));
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let object = FinalizedObject::new(
        ObjectRole::FileState,
        layerfs_content::object::codec::encode_bytes_object(&vec![47; 32000]).unwrap(),
    )
    .unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    let mut locations = Vec::new();
    handles
        .storage
        .locate(&[object.id()], &mut locations)
        .unwrap();
    let id = locations[0].pack.pack_id;
    let before = handles.diagnostics().unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let reader = {
        let provider = handles.storage.clone();
        std::thread::spawn(move || {
            provider
                .read_pack_selection(
                    id,
                    &mut HeldPlan {
                        entered: entered_tx,
                        release: release_rx,
                    },
                )
                .map(|_| ())
        })
    };
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let writer = {
        let provider = handles.storage.clone();
        std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            result_tx.send(provider.reserve(reserve())).unwrap();
        })
    };
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let premature = result_rx.recv_timeout(Duration::from_millis(100)).ok();
    let _ = release_tx.send(());
    let outcome = premature
        .clone()
        .unwrap_or_else(|| result_rx.recv_timeout(Duration::from_secs(3)).unwrap());
    assert!(reader.join().unwrap().is_ok());
    writer.join().unwrap();
    assert!(
        premature.is_none(),
        "own Session contention returned before admission: {premature:?}"
    );
    assert!(
        outcome.is_ok(),
        "one admitted reservation failed: {outcome:?}"
    );
    let after = handles.diagnostics().unwrap();
    assert_eq!(after.write_transactions - before.write_transactions, 1);
    assert_eq!(after.write_commits - before.write_commits, 1);
    assert_eq!(after.rollbacks, before.rollbacks);
}

#[cfg(target_os = "linux")]
struct Held {
    child: Child,
    socket: UnixStream,
}
#[cfg(target_os = "linux")]
impl Held {
    fn start(path: &Path, socket_path: &Path) -> Self {
        let listener = UnixListener::bind(socket_path).unwrap();
        listener.set_nonblocking(true).unwrap();
        // The pinned Linux proof image supplies Python; its socket deadline releases
        // this independently opened real kernel owner if the controller fails.
        let body = "import os,socket,fcntl\nf=os.open(os.path.dirname(os.environ['LAYERFS_ADMISSION_LOCK_CHILD']) or '.',os.O_RDONLY|os.O_DIRECTORY)\nfcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)\ns=socket.socket(socket.AF_UNIX)\ns.settimeout(10)\ns.connect(os.environ['LAYERFS_ADMISSION_LOCK_SOCKET'])\ns.sendall(b'L')\ntry:\n assert s.recv(1)==b'R'\nfinally:\n fcntl.flock(f,fcntl.LOCK_UN)\n os.close(f)\n";
        let mut child = Command::new("python3")
            .args(["-c", body])
            .env("LAYERFS_ADMISSION_LOCK_CHILD", path)
            .env("LAYERFS_ADMISSION_LOCK_SOCKET", socket_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::yield_now()
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("child readiness: {error}");
                }
            }
        };
        let mut held = Self { child, socket };
        held.socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        held.socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut ready = [0];
        held.socket.read_exact(&mut ready).unwrap();
        assert_eq!(ready, *b"L");
        held
    }
    fn release(mut self) {
        self.socket.write_all(b"R").unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "child exit deadline");
            std::thread::yield_now();
        }
    }
}
#[cfg(target_os = "linux")]
impl Drop for Held {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
#[cfg(target_os = "linux")]
fn another_process_admission_precedes_the_single_sqlite_begin() {
    // Keep the owned AF_UNIX path below Darwin's sockaddr_un capacity.
    let temp = support::Temp::new("wa");
    let path = temp.join("store.sqlite");
    let original = create(&path);
    let handles = open(&path);
    let before = handles.diagnostics().unwrap();
    let held = Held::start(&path, &temp.join("s"));
    let (ready_tx, ready_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let writer = {
        let provider = handles.storage.clone();
        std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            result_tx.send(provider.reserve(reserve())).unwrap();
        })
    };
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let premature = result_rx.recv_timeout(Duration::from_millis(100)).ok();
    held.release();
    let outcome = premature
        .clone()
        .unwrap_or_else(|| result_rx.recv_timeout(Duration::from_secs(3)).unwrap());
    writer.join().unwrap();
    assert!(
        premature.is_none(),
        "public reservation ignored the cooperative writer owner: {premature:?}"
    );
    assert!(
        outcome.is_ok(),
        "one admitted reservation failed: {outcome:?}"
    );
    let after = handles.diagnostics().unwrap();
    assert_eq!(after.write_transactions - before.write_transactions, 1);
    assert_eq!(after.write_commits - before.write_commits, 1);
    assert_eq!(after.rollbacks, before.rollbacks);
    drop(original);
}

#[test]
#[cfg(target_os = "linux")]
fn two_opened_handles_admit_eight_distinct_reservations_without_conflict() {
    let temp = support::Temp::new("writer-admission-two-handles");
    let path = temp.join("store.sqlite");
    let first = create(&path);
    let second = open(&path);
    let providers = [first.storage.clone(), second.storage.clone()];
    let (ready_tx, ready_rx) = mpsc::channel();
    let mut starts = Vec::new();
    let mut jobs = Vec::new();
    for index in 0..8 {
        let provider = providers[index % 2].clone();
        let ready = ready_tx.clone();
        let (start_tx, start_rx) = mpsc::channel();
        starts.push(start_tx);
        jobs.push(std::thread::spawn(move || {
            ready.send(()).unwrap();
            start_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            provider.reserve(reserve())
        }));
    }
    for _ in 0..8 {
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    for start in starts {
        start.send(()).unwrap();
    }
    let results = jobs
        .into_iter()
        .map(|job| job.join().unwrap())
        .collect::<Vec<_>>();
    assert!(
        results.iter().all(Result::is_ok),
        "own writer admission refused a peer: {results:?}"
    );
    let mut packs = results
        .iter()
        .map(|r| r.as_ref().unwrap().first_pack_id)
        .collect::<Vec<_>>();
    let mut ordinals = results
        .iter()
        .map(|r| r.as_ref().unwrap().first_ordinal)
        .collect::<Vec<_>>();
    packs.sort_unstable();
    packs.dedup();
    ordinals.sort_unstable();
    ordinals.dedup();
    assert_eq!(packs.len(), 8);
    assert_eq!(ordinals.len(), 8);
    // Neither open changed the required SQLite busy handler/profile.
    assert_eq!(first.profile().busy_timeout, 0);
    assert_eq!(second.profile().busy_timeout, 0);
}

/// Past the retired 2 s admission window (#352 F-040).
const LONG_HOLD: Duration = Duration::from_millis(2500);

/// Holds this Store's one connection turn through a public read until released.
fn hold_turn(
    handles: &Handles,
) -> (
    std::thread::JoinHandle<Result<(), PersistenceError>>,
    mpsc::Sender<()>,
) {
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let object = FinalizedObject::new(
        ObjectRole::FileState,
        layerfs_content::object::codec::encode_bytes_object(&vec![48; 32000]).unwrap(),
    )
    .unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    let mut locations = Vec::new();
    handles
        .storage
        .locate(&[object.id()], &mut locations)
        .unwrap();
    let id = locations[0].pack.pack_id;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let provider = handles.storage.clone();
    let holder = std::thread::spawn(move || {
        provider
            .read_pack_selection(
                id,
                &mut HeldPlan {
                    entered: entered_tx,
                    release: release_rx,
                },
            )
            .map(|_| ())
    });
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    (holder, release_tx)
}

#[test]
fn queued_writers_outlive_the_old_window_and_all_succeed() {
    let temp = support::Temp::new("writer-admission-long");
    let handles = create(&temp.join("store.sqlite"));
    let (holder, release) = hold_turn(&handles);
    let (ready_tx, ready_rx) = mpsc::channel();
    let writers = (0..4)
        .map(|_| {
            let provider = handles.storage.clone();
            let ready = ready_tx.clone();
            std::thread::spawn(move || {
                ready.send(()).unwrap();
                provider.reserve(reserve())
            })
        })
        .collect::<Vec<_>>();
    for _ in 0..4 {
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    std::thread::sleep(LONG_HOLD);
    release.send(()).unwrap();
    assert!(holder.join().unwrap().is_ok());
    let results = writers
        .into_iter()
        .map(|writer| writer.join().unwrap())
        .collect::<Vec<_>>();
    assert!(
        results.iter().all(Result::is_ok),
        "a queued writer failed instead of waiting its turn: {results:?}"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn a_cross_process_waiter_outlives_the_old_window_and_succeeds() {
    let temp = support::Temp::new("wl");
    let path = temp.join("store.sqlite");
    let original = create(&path);
    let handles = open(&path);
    let held = Held::start(&path, &temp.join("s"));
    let (result_tx, result_rx) = mpsc::channel();
    let writer = {
        let provider = handles.storage.clone();
        std::thread::spawn(move || result_tx.send(provider.reserve(reserve())).unwrap())
    };
    std::thread::sleep(LONG_HOLD);
    let premature = result_rx.try_recv().ok();
    held.release();
    let outcome = premature
        .clone()
        .unwrap_or_else(|| result_rx.recv_timeout(Duration::from_secs(3)).unwrap());
    writer.join().unwrap();
    assert!(
        premature.is_none() && outcome.is_ok(),
        "a writer queued behind another process failed: {premature:?} {outcome:?}"
    );
    drop(original);
}

fn refused(result: &Result<impl std::fmt::Debug, PersistenceError>, status: &str) -> bool {
    matches!(result, Err(PersistenceError::Refused { status: s }) if s == status)
}

#[test]
fn queued_writers_are_admitted_first_come_first_served() {
    let temp = support::Temp::new("writer-admission-fifo");
    let handles = create(&temp.join("store.sqlite"));
    let (holder, release) = hold_turn(&handles);
    let mut writers = Vec::new();
    for _ in 0..5 {
        let provider = handles.storage.clone();
        writers.push(std::thread::spawn(move || provider.reserve(reserve())));
        // Each writer is queued before the next one arrives.
        std::thread::sleep(Duration::from_millis(100));
    }
    release.send(()).unwrap();
    assert!(holder.join().unwrap().is_ok());
    let packs = writers
        .into_iter()
        .map(|writer| writer.join().unwrap().unwrap().first_pack_id)
        .collect::<Vec<_>>();
    assert!(
        packs.windows(2).all(|pair| pair[0] < pair[1]),
        "admission order differs from arrival order: {packs:?}"
    );
}

#[test]
fn closing_the_store_fails_every_queued_writer_typed_and_lets_the_holder_finish() {
    use layerfs_history::{HistoryCatalog, HistoryError};
    let temp = support::Temp::new("writer-admission-close");
    let handles = create(&temp.join("store.sqlite"));
    let (holder, release) = hold_turn(&handles);
    let (result_tx, result_rx) = mpsc::channel();
    let writers = (0..3)
        .map(|_| {
            let provider = handles.storage.clone();
            let result = result_tx.clone();
            std::thread::spawn(move || result.send(provider.reserve(reserve())).unwrap())
        })
        .collect::<Vec<_>>();
    std::thread::sleep(Duration::from_millis(200));
    handles.close();
    // Every queued writer wakes and fails while the holder still holds.
    for _ in 0..3 {
        let outcome = result_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(refused(&outcome, "Closed"), "{outcome:?}");
    }
    for writer in writers {
        writer.join().unwrap();
    }
    release.send(()).unwrap();
    assert!(holder.join().unwrap().is_ok());
    let later = handles.storage.reserve(reserve());
    assert!(refused(&later, "Closed"), "{later:?}");
    assert!(matches!(
        handles.history.publish_lane(),
        Err(HistoryError::OwnershipUnavailable)
    ));
}

#[test]
fn writers_queue_behind_a_held_publication_lane_without_deadlock() {
    use layerfs_history::HistoryCatalog;
    let temp = support::Temp::new("writer-admission-lane");
    let handles = create(&temp.join("store.sqlite"));
    let (result_tx, result_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let (held_tx, held_rx) = mpsc::channel();
        let (handles, result) = (&handles, result_tx.clone());
        scope.spawn(move || {
            let lane = handles.history.publish_lane().unwrap();
            held_tx.send(()).unwrap();
            // The holder's own write runs inside its held turn.
            result.send(handles.storage.reserve(reserve())).unwrap();
            std::thread::sleep(LONG_HOLD);
            drop(lane);
        });
        held_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        for _ in 0..3 {
            let result = result_tx.clone();
            scope.spawn(move || result.send(handles.storage.reserve(reserve())).unwrap());
        }
        let result = result_tx.clone();
        scope.spawn(move || {
            let lane = handles.history.publish_lane().unwrap();
            result.send(handles.storage.reserve(reserve())).unwrap();
            drop(lane);
        });
        for _ in 0..5 {
            let outcome = result_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            assert!(outcome.is_ok(), "{outcome:?}");
        }
    });
}

#[test]
#[cfg(target_os = "linux")]
fn a_holder_process_that_exits_hands_the_lock_to_the_waiter() {
    let temp = support::Temp::new("wx");
    let path = temp.join("store.sqlite");
    let original = create(&path);
    let handles = open(&path);
    let held = Held::start(&path, &temp.join("s"));
    let (result_tx, result_rx) = mpsc::channel();
    let provider = handles.storage.clone();
    let writer = std::thread::spawn(move || result_tx.send(provider.reserve(reserve())).unwrap());
    std::thread::sleep(Duration::from_millis(300));
    assert!(result_rx.try_recv().is_err(), "the writer ignored the lock");
    // Killing the owner releases its kernel lock; nothing is left behind.
    drop(held);
    let outcome = result_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    writer.join().unwrap();
    assert!(outcome.is_ok(), "{outcome:?}");
    drop(original);
}

#[test]
#[cfg(target_os = "linux")]
fn a_signalled_kernel_waiter_is_cancelled_and_leaves_the_queue_cleanly() {
    use nix::sys::{
        pthread,
        signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal},
    };
    extern "C" fn interrupt(_: i32) {}
    // No SA_RESTART: the signal interrupts the blocking flock with EINTR.
    let action = SigAction::new(
        SigHandler::Handler(interrupt),
        SaFlags::empty(),
        SigSet::empty(),
    );
    unsafe { signal::sigaction(Signal::SIGUSR1, &action) }.unwrap();
    let temp = support::Temp::new("wc");
    let path = temp.join("store.sqlite");
    let original = create(&path);
    let handles = open(&path);
    let held = Held::start(&path, &temp.join("s"));
    let (thread_tx, thread_rx) = mpsc::channel();
    let provider = handles.storage.clone();
    let first = std::thread::spawn(move || {
        thread_tx.send(pthread::pthread_self()).unwrap();
        provider.reserve(reserve())
    });
    let waiting = thread_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let provider = handles.storage.clone();
    let second = std::thread::spawn(move || provider.reserve(reserve()));
    std::thread::sleep(Duration::from_millis(300));
    pthread::pthread_kill(waiting, Signal::SIGUSR1).unwrap();
    let first = first.join().unwrap();
    assert!(refused(&first, "Cancelled"), "{first:?}");
    // The next waiter took the turn and waits in the kernel in its place.
    std::thread::sleep(Duration::from_millis(200));
    assert!(!second.is_finished(), "the second writer ignored the lock");
    held.release();
    let second = second.join().unwrap();
    assert!(second.is_ok(), "{second:?}");
    drop(original);
}

/// A read plan that asks for a write while its own read holds the turn.
struct ReentrantPlan {
    provider: std::sync::Arc<layerfs_persistence::StorageProvider>,
    outcome: Option<Result<Reserved, PersistenceError>>,
}
impl PackReadPlan for ReentrantPlan {
    fn select(&mut self, _: PackInfo, _: &[u8]) -> Result<PackReadChoice, PersistenceError> {
        self.outcome = Some(self.provider.reserve(reserve()));
        Ok(PackReadChoice::Whole)
    }
}

#[test]
fn a_thread_reentering_its_own_turn_is_refused_instead_of_deadlocking() {
    let temp = support::Temp::new("writer-admission-reentry");
    let handles = create(&temp.join("store.sqlite"));
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let object = FinalizedObject::new(
        ObjectRole::FileState,
        layerfs_content::object::codec::encode_bytes_object(&vec![49; 32000]).unwrap(),
    )
    .unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    let mut locations = Vec::new();
    handles
        .storage
        .locate(&[object.id()], &mut locations)
        .unwrap();
    let mut plan = ReentrantPlan {
        provider: handles.storage.clone(),
        outcome: None,
    };
    handles
        .storage
        .read_pack_selection(locations[0].pack.pack_id, &mut plan)
        .unwrap();
    assert!(
        matches!(plan.outcome, Some(Err(PersistenceError::Malformed))),
        "{:?}",
        plan.outcome
    );
    // The turn was released: the next call is admitted at once.
    assert!(handles.storage.reserve(reserve()).is_ok());
}
