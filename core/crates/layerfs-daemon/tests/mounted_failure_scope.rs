//! Real kernel mounts: how far one failure reaches (R6 proof track P2).
//!
//! - FP-27 at mount scope: one of the Store's two read sessions is put into
//!   an uncertain state the way `store_read_service.rs` does it, through the
//!   provider's public port with the provider's own answer, and cold files
//!   are then read through the mount, and through a sibling Workspace's
//!   mount to see how far the first failure reaches.
//! - P-1, the exhaustion half: a real external process holds the Store's
//!   SQLite writer while `touch` creates a name on a Workspace that has no
//!   local serial range. The low-water early refill of the P-1 ruling is not
//!   implemented and is not staged here.
//!
//! The Store is composed from the same public pieces `open_store` composes,
//! as `mounted_commit_failures.rs` does, so that the test keeps the provider
//! of each read session and counts reservations at the public History port.
//! Every call is forwarded to the real provider; nothing is injected.
//!
//! Product expectations are collected and reported after the mount has been
//! torn down, so a deviation still leaves its whole evidence. Counts only.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/held_writer.rs"]
mod held_writer;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mounted_commit.rs"]
mod rig;
use held_writer::HeldWriter;
use layerfs_bridge::control::{Reply, WorkspaceToken};
use layerfs_content::ObjectId;
use layerfs_daemon::store::{PortError, ReadLimits, Store, StoreReader};
use layerfs_history::HistoryCatalog;
use layerfs_persistence::{Handles, SqlitePersistenceProfile, StorageProvider};
use layerfs_storage::{
    location::PackInfo,
    port::{PackPersistence, PackReadChoice, PackReadPlan, PersistenceError},
    ReservationBlocks, Storage,
};
use model::Model;
use mounted::{mount_entry, Harness, COMMAND};
use nix::errno::Errno;
use rig::{bash_in, pattern, root, stamp_tree};
use std::{
    fmt::Debug,
    fs,
    io::{self, Read},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command as Process,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(8);

/// The real catalog, with every serial reservation and its answer recorded.
mod counted {
    use layerfs_history::*;
    use layerfs_persistence::Handles;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    pub struct Attempt {
        /// The reserved range as `(start, count)`.
        pub reserved: Option<(u64, u64)>,
        /// The provider answered `Busy`: no write effect.
        pub busy: bool,
        /// The provider's original answer.
        pub answer: String,
    }
    impl std::fmt::Debug for Attempt {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.answer)
        }
    }
    pub struct CountedHistory {
        pub handles: Arc<Handles>,
        pub attempts: Mutex<Vec<Attempt>>,
    }
    macro_rules! forward {
        ($($name:ident($($arg:ident: $ty:ty),*) -> $out:ty;)*) => {$(
            fn $name(&self, $($arg: $ty),*) -> $out { self.handles.history.$name($($arg),*) }
        )*};
    }
    impl HistoryCatalog for CountedHistory {
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
            stage_and_commit(request: &StageRequest) -> HistoryResult<CommitStagedOutcome>;
            add_layer(request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome>;
            discard_stage(request: &DiscardRequest) -> HistoryResult<DiscardOutcome>;
        }
        fn reserve_inodes(&self, request: &ReserveRequest) -> HistoryResult<Reservation> {
            let result = self.handles.history.reserve_inodes(request);
            self.attempts.lock().unwrap().push(Attempt {
                reserved: result.as_ref().ok().map(|range| (range.start, range.count)),
                busy: matches!(result, Err(HistoryError::Busy)),
                answer: match &result {
                    Ok(range) => format!("Ok(start={} count={})", range.start, range.count),
                    Err(error) => format!("Err({error:?})"),
                },
            });
            result
        }
    }
}
use counted::{Attempt, CountedHistory};

/// A bounded description of an original value for an evidence line.
fn brief(value: &impl Debug) -> String {
    let mut text = format!("{value:?}");
    let mut end = text.len().min(480);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}
/// Product expectations that did not hold. Reported after teardown.
#[derive(Default)]
struct Checks(Vec<String>);
impl Checks {
    fn that(&mut self, holds: bool, what: impl FnOnce() -> String) {
        if !holds {
            let what = what();
            println!("P2_DEVIATION {what}");
            self.0.push(what);
        }
    }
    fn done(self, name: &str) {
        assert!(
            self.0.is_empty(),
            "{name}: {} product expectations did not hold: {:#?}",
            self.0.len(),
            self.0
        );
    }
}
macro_rules! expect {
    ($checks:expr, $holds:expr, $($what:tt)+) => {
        $checks.that($holds, || format!($($what)+))
    };
}
/// Never leaves a kernel mount behind: if the test unwinds, or the product
/// did not unmount, one lazy umount(8) launched and reaped by the test
/// detaches what is still there.
struct Mounted(String);
impl Drop for Mounted {
    fn drop(&mut self) {
        if mount_entry(&self.0).is_some() {
            let done = Process::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "P2_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}
/// A bounded observation loop that reports instead of unwinding.
fn eventually(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(2));
    }
}
/// Two consecutive observations of a connection with nothing received or
/// admitted and no request completed in between.
fn quiet(harness: &Harness, token: WorkspaceToken) -> bool {
    let mut last = None;
    eventually(|| {
        let work = harness.status(token).native.unwrap().work.unwrap();
        let now = (work.received, work.admitted, work.completed);
        let settled = work.received == 0 && work.admitted == 0 && last == Some(now);
        last = Some(now);
        settled
    })
}

/// Files none of which a bind reads: every first read is a cold demand.
const COLD: [(&str, u8, usize); 8] = [
    ("cold-0.bin", 20, 48_000),
    ("cold-1.bin", 21, 49_000),
    ("cold-2.bin", 22, 50_000),
    ("cold-3.bin", 23, 51_000),
    ("cold-4.bin", 24, 52_000),
    ("cold-5.bin", 25, 53_000),
    ("cold-6.bin", 26, 54_000),
    ("cold-7.bin", 27, 55_000),
];
fn cold_tree(source: &Path) {
    fs::write(source.join("README.md"), b"# base\n").unwrap();
    for (name, seed, length) in COLD {
        fs::write(source.join(name), pattern(seed, length)).unwrap();
    }
    for entry in fs::read_dir(source).unwrap() {
        fs::set_permissions(entry.unwrap().path(), fs::Permissions::from_mode(0o644)).unwrap();
    }
    stamp_tree(source);
}

/// One installed Disposable Store, opened once and kept open, and one real
/// native serving assembly over it. The test keeps the writable session, the
/// provider of each read session and the counted catalog.
struct Fx {
    fixture: installed::Fixture,
    store: Arc<Store>,
    harness: Harness,
    writer: Arc<Handles>,
    history: Arc<CountedHistory>,
    readers: Vec<Arc<StorageProvider>>,
}
impl Fx {
    fn new(label: &str) -> Self {
        let fixture = installed::Fixture::built(label, |source| {
            cold_tree(source);
            Model::native(source).1
        });
        assert!(
            matches!(
                fixture.config.sqlite_profile,
                SqlitePersistenceProfile::Disposable
            ),
            "the global Store profile is Disposable, selected explicitly"
        );
        let config = || fixture.config.clone();
        let writer = Arc::new(
            Handles::open_writable(config(), installed::BINDING, installed::CURSOR).unwrap(),
        );
        let history = Arc::new(CountedHistory {
            handles: writer.clone(),
            attempts: Mutex::new(Vec::new()),
        });
        let mut readers = Vec::new();
        let sessions = (0..2)
            .map(|_| {
                let read = Handles::open_read_only(config(), installed::BINDING, installed::CURSOR)
                    .unwrap();
                readers.push(read.storage.clone());
                StoreReader::new(Storage::new(read.storage).unwrap(), Arc::new(read.history))
            })
            .collect();
        let pack: Arc<dyn PackPersistence> = writer.storage.clone();
        let catalog: Arc<dyn HistoryCatalog> = history.clone();
        let store = Arc::new(
            Store::new(
                pack,
                catalog,
                sessions,
                2 * 1024 * 1024,
                ReservationBlocks::default(),
                ReadLimits::default(),
            )
            .unwrap(),
        );
        let harness = Harness::new(store.clone(), &fixture.directory, fixture.branch);
        Self {
            fixture,
            store,
            harness,
            writer,
            history,
            readers,
        }
    }
    fn attempts(&self) -> Vec<Attempt> {
        self.history.attempts.lock().unwrap().clone()
    }
    /// One normal Unmount of a connection the test left quiet. True when it
    /// answered `Unmounted`; its answer is printed either way.
    fn unmount(&self, token: WorkspaceToken, what: &str) -> bool {
        let quiet = quiet(&self.harness, token);
        let closed = self.harness.try_unmount(token);
        let clean = matches!(&closed, Ok(done) if done.reply == Reply::Unmounted(token));
        println!(
            "P2_UNMOUNT {what}: quiet_before={quiet} unmounted={clean} answer={}",
            match &closed {
                Ok(done) => brief(&done.reply),
                Err(failure) => brief(failure),
            }
        );
        clean
    }
    /// `clean`: no mount and no custody remains, so the serving assembly is
    /// stopped with its checks. Otherwise the registry still owns what the
    /// product kept, and the assembly is dropped with it.
    fn finish(self, clean: bool) {
        let Self {
            fixture,
            store,
            harness,
            writer,
            history,
            readers,
        } = self;
        if clean {
            harness.stop();
        } else {
            let Harness {
                owner,
                serving,
                service,
                ..
            } = harness;
            drop(service);
            drop(serving);
            println!(
                "P2_RIG owner stopped without a clean drain: {:?}",
                owner.stop()
            );
        }
        drop((history, readers, writer));
        println!(
            "P2_RIG store_opened=1 sealed=0 reopened=0 profile=Disposable clean_stop={clean} handles_after_stop={}",
            Arc::strong_count(&store)
        );
        drop(store);
        fixture.cleanup();
    }
}

// ---------------------------------------------------------------- FP-27

/// The read session the test puts out of service.
const QUARANTINED: usize = 0;
struct Uncertain;
impl PackReadPlan for Uncertain {
    fn select(&mut self, _: PackInfo, _: &[u8]) -> Result<PackReadChoice, PersistenceError> {
        Err(PersistenceError::Uncertain)
    }
}
/// As `store_read_service.rs`: one scoped pack read on the session's own
/// provider whose plan answers `Uncertain`, which the provider keeps.
fn quarantine(provider: &StorageProvider, root: ObjectId) {
    let mut rows = Vec::new();
    provider.locate(&[root], &mut rows).unwrap();
    assert!(matches!(
        provider.read_scoped_pack(rows[0].location.pack_id, &mut Uncertain),
        Err(PersistenceError::Uncertain)
    ));
}
/// One whole-file read by ordinary syscalls: the failing step and its error.
fn read_once(path: &Path) -> Result<Vec<u8>, (&'static str, io::Error)> {
    let mut file = fs::File::open(path).map_err(|error| ("open", error))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| ("read", error))?;
    Ok(bytes)
}
type Answer = Result<Vec<u8>, (&'static str, io::Error)>;
/// The read runs on its own thread, which ends when its syscalls return.
fn reading(path: PathBuf) -> mpsc::Receiver<Answer> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(read_once(&path));
    });
    receiver
}
/// What one read of a cold file gave.
#[derive(Debug, Eq, PartialEq)]
enum Seen {
    Exact,
    Wrong {
        bytes: usize,
    },
    Failed {
        step: &'static str,
        errno: Option<Errno>,
    },
    /// No answer inside the bound.
    Unanswered,
}
fn seen(answer: Option<Answer>, expected: &[u8]) -> Seen {
    match answer {
        None => Seen::Unanswered,
        Some(Ok(bytes)) if bytes == expected => Seen::Exact,
        Some(Ok(bytes)) => Seen::Wrong { bytes: bytes.len() },
        Some(Err((step, error))) => Seen::Failed {
            step,
            errno: error.raw_os_error().map(Errno::from_raw),
        },
    }
}

#[test]
fn a_quarantined_reader_gets_no_further_demand_and_one_cold_failure_fails_no_later_read() {
    let fx = Fx::new("fp-27");
    let ready = fx.harness.mount(1);
    let mounted = Mounted(ready.directory.clone());
    let mount = root(&ready).to_path_buf();
    let token = ready.token;
    // A sibling Workspace of the same Branch, mounted before anything fails.
    let sibling = fx.harness.mount(2);
    let sibling_mounted = Mounted(sibling.directory.clone());
    let mut checks = Checks::default();
    let base = fx
        .store
        .history()
        .branch_snapshot(fx.fixture.branch)
        .unwrap()
        .unwrap()
        .effective_root;
    let demand = |fx: &Fx| {
        fx.readers[QUARANTINED]
            .diagnostics()
            .map(|work| work.statements)
    };

    // Nothing is in flight. The test takes the other read session, so the
    // next demand of the mount can only be given the one put out of service.
    assert!(quiet(&fx.harness, token), "the fresh mount is quiet");
    assert!(quiet(&fx.harness, sibling.token), "the sibling is quiet");
    let idle = fx.store.read_work();
    assert_eq!(
        (idle.readers, idle.quarantined, idle.outstanding),
        (2, 0, 0),
        "{idle:?}"
    );
    let first = fx.store.read_ticket(None).unwrap().wait().unwrap();
    let healthy = if first.index() == QUARANTINED {
        let second = fx.store.read_ticket(None).unwrap().wait().unwrap();
        drop(first);
        second
    } else {
        first
    };
    assert_eq!(healthy.index(), 1 - QUARANTINED);
    let before_staging = demand(&fx).unwrap();
    quarantine(&fx.readers[QUARANTINED], base);
    let staged = demand(&fx).unwrap();
    let demands_before = fx.store.work();

    // The observing request: the first cold read through the mount.
    let expected = |index: usize| pattern(COLD[index].1, COLD[index].2);
    let observing = reading(mount.join(COLD[0].0));
    let retired = eventually(|| fx.store.read_work().quarantined == 1);
    let at_retirement = demand(&fx);
    let pool_at_retirement = fx.store.read_work();
    drop(healthy);
    let observed = seen(observing.recv_timeout(WAIT).ok(), &expected(0));
    let failures = fx.store.reader_failures();
    let work_after_first = fx
        .harness
        .status(token)
        .native
        .and_then(|native| native.work);
    println!(
        "FP_27 staged: reader={QUARANTINED} statements_before_staging={before_staging} after_staging={staged} | observing read of {}: {observed:?} | quarantined_observed={retired} pool={pool_at_retirement:?} reader_statements_at_retirement={at_retirement:?}",
        COLD[0].0
    );
    println!(
        "FP_27 reader_failures={} store_demands_before={demands_before:?} after={:?} native_work_after_observing_read={work_after_first:?}",
        brief(&failures),
        fx.store.work()
    );
    expect!(
        checks,
        retired,
        "quarantined is {} after the observing read, not 1: {pool_at_retirement:?}",
        pool_at_retirement.quarantined
    );
    expect!(
        checks,
        failures.len() == 1
            && failures[0].0 == QUARANTINED
            && matches!(failures[0].1.as_ref(), PortError::Storage(error) if error.is_unknown_outcome()),
        "the retired reader and its original cause: {}",
        brief(&failures)
    );
    expect!(
        checks,
        matches!(
            observed,
            Seen::Exact
                | Seen::Failed {
                    errno: Some(Errno::EIO),
                    ..
                }
        ),
        "the observing read neither succeeded nor failed with EIO: {observed:?}"
    );

    // Later requests: every other cold file, then the first one again.
    let mut later = Vec::new();
    for index in (1..COLD.len()).chain([0]) {
        let answer = seen(
            reading(mount.join(COLD[index].0)).recv_timeout(WAIT).ok(),
            &expected(index),
        );
        let stop = answer == Seen::Unanswered;
        later.push((COLD[index].0, answer));
        if stop {
            break;
        }
    }
    let exact = later
        .iter()
        .filter(|(_, answer)| *answer == Seen::Exact)
        .count();
    println!(
        "FP_27 later reads: exact={exact} of {} attempted ({} planned): {later:?}",
        later.len(),
        COLD.len()
    );
    expect!(
        checks,
        later.len() == COLD.len() && exact == later.len(),
        "{} of {} later reads were not exact: {:?}",
        COLD.len() - exact,
        COLD.len(),
        later
            .iter()
            .filter(|(_, answer)| *answer != Seen::Exact)
            .collect::<Vec<_>>()
    );
    let fenced = later.iter().any(|(_, answer)| {
        matches!(
            answer,
            Seen::Failed {
                errno: Some(Errno::ENOTCONN),
                ..
            }
        )
    });

    // The same cold files through the sibling's mount: its own kernel cache
    // holds none of them and no read of the first mount cached one.
    let demands_before_sibling = fx.store.work();
    let grants_before_sibling = fx.store.read_work().grants;
    // The later reads on the first mount were real cold demands, served by
    // the readers that are left.
    expect!(
        checks,
        demands_before_sibling.object_batches > demands_before.object_batches,
        "the later reads on the first mount made no Store object demand: {demands_before:?} then {demands_before_sibling:?}"
    );
    let mut beside = Vec::new();
    for (index, (name, _, _)) in COLD.iter().enumerate() {
        let answer = seen(
            reading(root(&sibling).join(name)).recv_timeout(WAIT).ok(),
            &expected(index),
        );
        let stop = answer == Seen::Unanswered;
        beside.push((*name, answer));
        if stop {
            break;
        }
    }
    let demands_after_sibling = fx.store.work();
    let grants_after_sibling = fx.store.read_work().grants;
    let exact_beside = beside
        .iter()
        .filter(|(_, answer)| *answer == Seen::Exact)
        .count();
    println!(
        "FP_27 sibling mount: exact={exact_beside} of {} attempted ({} planned), Store object demands {} -> {}, length demands {} -> {}, reader grants {grants_before_sibling} -> {grants_after_sibling}, quarantined reader statements {:?}: {:?}",
        beside.len(),
        COLD.len(),
        demands_before_sibling.object_batches,
        demands_after_sibling.object_batches,
        demands_before_sibling.length_batches,
        demands_after_sibling.length_batches,
        demand(&fx),
        beside
            .iter()
            .filter(|(_, answer)| *answer != Seen::Exact)
            .collect::<Vec<_>>()
    );
    expect!(
        checks,
        beside.len() == COLD.len() && exact_beside == beside.len(),
        "{} of {} reads through the sibling mount were not exact",
        COLD.len() - exact_beside,
        COLD.len()
    );
    // Since the first mount keeps serving after its one failed demand, its
    // own later reads have already put these objects, and the lengths of
    // these files, in the cache both Workspaces of the Branch share. Neither
    // object nor length demands need rise for the sibling (`U-attempt8` is
    // the receipt of the earlier object-only expectation failing, and
    // `341-read-s2-linux-daemon` that of the length one). What its reads
    // still take is a Store reader for every window of base bytes, from the
    // readers that are left.
    expect!(
        checks,
        grants_after_sibling - grants_before_sibling >= COLD.len() as u64,
        "the sibling's {} reads took {} Store readers: {demands_before_sibling:?} then {demands_after_sibling:?}",
        COLD.len(),
        grants_after_sibling - grants_before_sibling
    );
    let at_end = demand(&fx);
    let pool = fx.store.read_work();
    let status = fx.harness.status(token);
    let work = status.native.as_ref().and_then(|native| native.work);
    println!(
        "FP_27 end: reader_statements={at_end:?} (at retirement {at_retirement:?}) pool={pool:?} later_read_saw_ENOTCONN={fenced} activity={:?} phase={:?} native_work={work:?}",
        status.activity,
        status.native.as_ref().map(|native| native.phase)
    );
    expect!(
        checks,
        matches!((&at_retirement, &at_end), (Ok(then), Ok(now)) if then == now),
        "the quarantined reader's statement count moved or could not be read: {at_retirement:?} then {at_end:?}"
    );
    expect!(
        checks,
        pool.quarantined == 1 && pool.readers == 2,
        "the read pool at the end: {pool:?}"
    );

    let clean_sibling = fx.unmount(sibling.token, "FP-27 sibling");
    drop(sibling_mounted);
    let clean = fx.unmount(token, "FP-27");
    drop(mounted);
    fx.finish(clean && clean_sibling);
    checks.done("FP-27 at mount scope");
}

// ---------------------------------------------------------------- P-1

fn names(mount: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(mount)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_create_with_no_serial_range_under_a_held_store_writer_is_eagain_with_no_effect() {
    let fx = Fx::new("p-1");
    let ready = fx.harness.mount(1);
    let mounted = Mounted(ready.directory.clone());
    let mount = root(&ready).to_path_buf();
    let token = ready.token;
    let mut checks = Checks::default();
    let revision = |fx: &Fx| {
        fx.harness
            .status(token)
            .local
            .map(|local| (local.revision, local.dirty_inodes))
    };

    // No serial was reserved since the Store was opened, so the Workspace
    // has no local range: its first create must reserve.
    let opened = (fx.attempts().len(), fx.store.work().serial_reservations);
    assert_eq!(opened, (0, 0), "the Workspace starts with no serial range");
    let names_before = names(&mount);
    let revision_before = revision(&fx);

    // A real external process holds the Store's writer.
    let held = HeldWriter::acquire(&fx.fixture.config.path);
    let sql_before = fx.writer.diagnostics().unwrap();
    let refused = bash_in(COMMAND, &mount, "touch contended.txt");
    let sql_held = fx.writer.diagnostics().unwrap();
    let attempts_held = fx.attempts();
    let counted_held = fx.store.work().serial_reservations;
    let lookup_held = fs::symlink_metadata(mount.join("contended.txt")).map(|_| ());
    let names_held = names(&mount);
    let revision_held = revision(&fx);
    let work_held = fx
        .harness
        .status(token)
        .native
        .and_then(|native| native.work);
    held.release();
    let stderr = String::from_utf8_lossy(&refused.stderr).into_owned();
    println!(
        "P_1 held writer: touch status={:?} stderr={:?} | reservation attempts={attempts_held:?} store_serial_reservations={counted_held} | writer session statements {} -> {} write_transactions {} -> {} | name lookup={:?} names={names_held:?} | engine (revision, dirty_inodes) {revision_before:?} -> {revision_held:?} | native_work={work_held:?}",
        refused.status.code(),
        stderr.trim(),
        sql_before.statements,
        sql_held.statements,
        sql_before.write_transactions,
        sql_held.write_transactions,
        lookup_held.as_ref().map_err(io::Error::kind)
    );
    expect!(
        checks,
        !refused.status.success() && stderr.contains("Resource temporarily unavailable"),
        "touch under the held writer did not fail with EAGAIN: {:?} {stderr:?}",
        refused.status
    );
    expect!(
        checks,
        matches!(&lookup_held, Err(error) if error.kind() == io::ErrorKind::NotFound)
            && names_held == names_before,
        "the refused name is not absent: {lookup_held:?} {names_held:?}"
    );
    expect!(
        checks,
        attempts_held.len() == 1 && attempts_held[0].busy && attempts_held[0].reserved.is_none(),
        "not exactly one reservation attempt answered Busy: {attempts_held:?}"
    );
    expect!(
        checks,
        counted_held == 1,
        "the Store counted {counted_held} serial reservations for one create"
    );
    expect!(
        checks,
        sql_held.write_transactions == sql_before.write_transactions,
        "the writer session ran a write transaction under the held writer: {} -> {}",
        sql_before.write_transactions,
        sql_held.write_transactions
    );
    expect!(
        checks,
        revision_held.is_some() && revision_held == revision_before,
        "the refused create changed the engine: {revision_before:?} -> {revision_held:?}"
    );
    expect!(
        checks,
        work_held.is_some_and(|work| work.retained == 0 && work.terminal == 0),
        "the refused create left the connection retained or terminal: {work_held:?}"
    );

    // The writer is released: a later create reserves and succeeds, and the
    // one after it uses the local range.
    let created = bash_in(COMMAND, &mount, "touch contended.txt");
    let attempts_created = fx.attempts();
    let made = fs::symlink_metadata(mount.join("contended.txt"));
    let another = bash_in(COMMAND, &mount, "touch another.txt");
    let attempts_after = fx.attempts();
    let names_after = names(&mount);
    let sql_after = fx.writer.diagnostics().unwrap();
    println!(
        "P_1 released: touch status={:?} stderr={:?} | reservation attempts={attempts_created:?} | second create status={:?} attempts={} store_serial_reservations={} | writer session write_transactions {} -> {} | names={names_after:?}",
        created.status.code(),
        String::from_utf8_lossy(&created.stderr).trim(),
        another.status.code(),
        attempts_after.len(),
        fx.store.work().serial_reservations,
        sql_held.write_transactions,
        sql_after.write_transactions
    );
    expect!(
        checks,
        created.status.success(),
        "touch after the writer was released: {:?} {:?}",
        created.status,
        String::from_utf8_lossy(&created.stderr)
    );
    expect!(
        checks,
        attempts_created.len() == 2 && attempts_created[1].reserved.is_some(),
        "the later create did not make one successful reservation: {attempts_created:?}"
    );
    expect!(
        checks,
        matches!(&made, Ok(made) if made.is_file() && made.len() == 0 && made.uid() == COMMAND),
        "the created name: {made:?}"
    );
    expect!(
        checks,
        another.status.success() && attempts_after.len() == 2,
        "a create inside the reserved range: {:?}, {} attempts",
        another.status,
        attempts_after.len()
    );
    let mut expected = names_before.clone();
    expected.extend(["another.txt".to_owned(), "contended.txt".to_owned()]);
    expected.sort();
    expect!(
        checks,
        names_after == expected,
        "the directory after both creates: {names_after:?}"
    );

    let clean = fx.unmount(token, "P-1");
    drop(mounted);
    fx.finish(clean);
    checks.done("P-1 exhaustion half");
}
