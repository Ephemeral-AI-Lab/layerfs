//! Real kernel mounts: every product control Commit has one construction
//! producer, the one Store session stays open across Commits, and that
//! writer session's own read-back profile is Disposable / WAL /
//! synchronous=OFF (row R5-10).
//!
//! The Store is composed from the same public pieces `open_store` composes,
//! so that the writable `Handles` stay in the test's hands: `open_store`
//! keeps its writer private. Two things are observed that the product's own
//! receipt cannot show alone:
//!
//! - The writer port the Store was given is a pure forwarding observer. It
//!   records, for every call, the calling thread and this process's thread
//!   count, and forwards the call unchanged. The public receipt
//!   (`CommitSuccess.storage`) counts within one producer handle, so a second
//!   producer would not appear in it; the observer counts `policy()` reads of
//!   the shared writer, one per producer handle.
//! - The profile is read from the writable session itself
//!   (`Handles::profile()`), not from a separate observer session. It is the
//!   read-back that session made when it opened.
//!
//! `LAYERFS_CONSTRUCTION_WORKERS` is not read by any file under
//! `core/crates/*/src`; nothing here can make it select anything. The bound
//! shown is the one the product has by construction.
//!
//! One test: the thread count is a process-wide observation.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/history_gate.rs"]
mod history_gate;
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
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use history_gate::Point;
use layerfs_bridge::control::{Activity, Reply, Request, WorkspaceToken};
use layerfs_content::ObjectId;
use layerfs_daemon::store::{ReadLimits, Store, StoreReader};
use layerfs_history::{CommitRecord, CommitStagedOutcome, HistoryCatalog, WorkspaceId};
use layerfs_persistence::{Handles, SqlitePersistenceProfile};
use layerfs_storage::{
    location::{LocatedObject, SignatureRow},
    port::{
        AcquiredPackRead, PackPersistence, PackReadPlan, PersistedPack, PersistedPackRead,
        PersistenceError, Publication, Published, PublishedPack, Reserve, Reserved,
        ValueGroupQuery, ValueGroups,
    },
    ReservationBlocks, Storage, StoragePolicy,
};
use model::{assert_same, Flat, Model};
use mounted::{until, Harness, COMMAND};
use rig::{base_tree, bash_in, native, passed, root};
use std::{
    cell::Cell,
    collections::BTreeSet,
    fs,
    os::unix::fs::MetadataExt,
    path::Path,
    sync::{Arc, Mutex},
    thread::{self, ThreadId},
    time::Duration,
};

/// Which writer-port call was made.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Port {
    Policy,
    Locate,
    ReadPacks,
    ReadPackSelection,
    ReadScopedPack,
    ValueGroups,
    Signatures,
    Reserve,
    PackCost,
    Publish,
}
#[derive(Clone, Copy, Debug)]
struct Seen {
    port: Port,
    thread: ThreadId,
    /// Threads of this process when the call arrived.
    threads: usize,
}
/// This process's threads, as the kernel lists them.
fn threads() -> usize {
    fs::read_dir("/proc/self/task").unwrap().count()
}
/// The real writer session behind a recorder. Every call is forwarded
/// unchanged and its answer returned unchanged.
struct Watch {
    inner: Arc<dyn PackPersistence>,
    seen: Mutex<Vec<Seen>>,
}
impl Watch {
    fn note(&self, port: Port) {
        self.seen
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(Seen {
                port,
                thread: thread::current().id(),
                threads: threads(),
            });
    }
    /// Every call recorded since the previous take.
    fn take(&self) -> Vec<Seen> {
        std::mem::take(&mut *self.seen.lock().unwrap_or_else(|error| error.into_inner()))
    }
}
type Answer<T> = Result<T, PersistenceError>;
impl PackPersistence for Watch {
    fn policy(&self) -> Answer<StoragePolicy> {
        self.note(Port::Policy);
        self.inner.policy()
    }
    fn locate(&self, ids: &[ObjectId], out: &mut Vec<LocatedObject>) -> Answer<()> {
        self.note(Port::Locate);
        self.inner.locate(ids, out)
    }
    fn read_packs(&self, ids: &[i64], out: &mut Vec<PersistedPack>) -> Answer<()> {
        self.note(Port::ReadPacks);
        self.inner.read_packs(ids, out)
    }
    fn read_pack_selection(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Answer<PersistedPackRead> {
        self.note(Port::ReadPackSelection);
        self.inner.read_pack_selection(id, plan)
    }
    fn read_scoped_pack(&self, id: i64, plan: &mut dyn PackReadPlan) -> Answer<AcquiredPackRead> {
        self.note(Port::ReadScopedPack);
        self.inner.read_scoped_pack(id, plan)
    }
    fn value_groups(&self, query: ValueGroupQuery<'_>) -> Answer<ValueGroups> {
        self.note(Port::ValueGroups);
        self.inner.value_groups(query)
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Answer<()> {
        self.note(Port::Signatures);
        self.inner.signatures(out)
    }
    fn reserve(&self, request: Reserve) -> Answer<Reserved> {
        self.note(Port::Reserve);
        self.inner.reserve(request)
    }
    fn publication_pack_cost(&self, pack: &PublishedPack) -> Answer<(usize, u64)> {
        self.note(Port::PackCost);
        self.inner.publication_pack_cost(pack)
    }
    fn publish(&self, batch: &Publication) -> Answer<Published> {
        self.note(Port::Publish);
        self.inner.publish(batch)
    }
}

/// Two consecutive observations of a connection with nothing received or
/// admitted and no request completed in between. A readiness wait before one
/// attempt, never a repeated attempt.
fn quiet(harness: &Harness, token: WorkspaceToken) {
    let mut last = None;
    until("connection quiescent", || {
        let work = harness.status(token).native.unwrap().work.unwrap();
        let now = (work.received, work.admitted, work.completed);
        let settled = work.received == 0 && work.admitted == 0 && last == Some(now);
        last = Some(now);
        settled
    });
}
/// The Branch's current root, which must be `root`, walked completely through
/// a fresh bind of a new Workspace that is closed again.
fn published(harness: &Harness, fresh: &Cell<u8>, root: ObjectId) -> Flat {
    let tag = fresh.get();
    fresh.set(tag.checked_add(1).expect("fresh bind tags"));
    let bound = harness
        .service
        .execute_control(&Request::Mount {
            workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
            branch: harness.branch,
        })
        .unwrap();
    let token = match &bound.reply {
        Reply::Bound { token, binding } => {
            assert_eq!(
                binding.effective_root, root,
                "a fresh bind selects the published root"
            );
            *token
        }
        other => panic!("{other:?}"),
    };
    drop(bound);
    let operation = harness.service.operation(token).unwrap();
    let flat = model::canonical(operation.client(), root);
    assert!(operation.ports().failure().unwrap().is_none());
    drop(operation);
    let closed = harness.try_unmount(token).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(token));
    flat
}
/// The writer's database file and its write-ahead log, as the kernel has them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Files {
    database: (u64, u64),
    log: (u64, u64),
}
fn files(database: &Path) -> Files {
    let identity = |path: &Path| {
        let metadata = fs::metadata(path).unwrap_or_else(|error| panic!("{path:?}: {error}"));
        (metadata.dev(), metadata.ino())
    };
    let mut log = database.as_os_str().to_owned();
    log.push("-wal");
    Files {
        database: identity(database),
        log: identity(Path::new(&log)),
    }
}

/// The changes of each Commit, by externally launched bash in the mount.
const ROUNDS: [&str; 4] = [
    r#"set -euo pipefail; umask 022
mkdir -p made/one
printf 'first round\n' > made/one/a.txt
printf 'appended in round one\n' >> README.md"#,
    r#"set -euo pipefail; umask 022
printf 'OVERWRITTEN' | dd of=target/debug/app bs=1 seek=200000 conv=notrunc status=none
chmod 0600 src/lib.rs
rm -f spare/gone.txt"#,
    r#"set -euo pipefail; umask 022
mv -T made/one/a.txt made/moved.txt
ln -s moved.txt made/link
head -c 150000 /dev/zero | tr '\0' 'r' > made/large.bin"#,
    r#"set -euo pipefail; umask 022
printf 'made on the second mount\n' > made/second-mount.txt
truncate -s 77 made/large.bin"#,
];

/// R5-10.
#[test]
fn r5_10_each_mounted_commit_has_one_producer_on_the_one_open_disposable_writer_session() {
    let name = "R5-10";
    let fixture = installed::Fixture::built("r5-10-producer", |source| {
        base_tree(source);
        Model::native(source).1
    });
    // The profile is selected explicitly, and it is Disposable.
    assert_eq!(
        fixture.config.sqlite_profile,
        SqlitePersistenceProfile::Disposable
    );
    let config = || fixture.config.clone();
    let database = fixture.config.path.clone();

    // The one writable session of the Store, kept by the test.
    let handles =
        Arc::new(Handles::open_writable(config(), installed::BINDING, installed::CURSOR).unwrap());
    // Its own read-back: what this session read from itself when it opened.
    let profile = handles.profile().clone();
    assert_eq!(profile.persistence, SqlitePersistenceProfile::Disposable);
    assert_eq!(profile.journal_mode, "wal");
    assert_eq!(profile.synchronous, 0, "synchronous=OFF");
    assert_eq!(profile.fullfsync, 0);
    println!(
        "{name} WRITER_PROFILE session=writable(Handles::open_writable) selected={:?} readback(persistence={:?} identity={} journal_mode={} synchronous={} fullfsync={} mmap_size={} busy_timeout={} sqlite={}) durable=NOT_RUN",
        fixture.config.sqlite_profile,
        profile.persistence,
        profile.identity,
        profile.journal_mode,
        profile.synchronous,
        profile.fullfsync,
        profile.mmap_size,
        profile.busy_timeout,
        profile.sqlite_version
    );

    // The Store over that session: the writer port behind the recorder, the
    // real catalog behind a gate that is released before anything uses it and
    // so only forwards.
    let watch = Arc::new(Watch {
        inner: handles.storage.clone(),
        seen: Mutex::new(Vec::new()),
    });
    let (catalog, hold) =
        history_gate::gated(handles.clone(), Point::BeforePublication, Duration::ZERO);
    hold.release();
    let history: Arc<dyn HistoryCatalog> = Arc::new(catalog);
    let readers = (0..2)
        .map(|_| {
            let read =
                Handles::open_read_only(config(), installed::BINDING, installed::CURSOR).unwrap();
            StoreReader::new(Storage::new(read.storage).unwrap(), Arc::new(read.history))
        })
        .collect();
    let store = Arc::new(
        Store::new(
            watch.clone(),
            history,
            readers,
            2 * 1024 * 1024,
            ReservationBlocks::default(),
            ReadLimits::default(),
        )
        .unwrap(),
    );
    let opened = Arc::as_ptr(&store);
    let harness = Harness::new(store.clone(), &fixture.directory, fixture.branch);
    // Composing the Store read the writer's policy once and nothing else.
    let composing = watch.take();
    assert_eq!(
        composing.iter().map(|seen| seen.port).collect::<Vec<_>>(),
        [Port::Policy]
    );
    let fresh = Cell::new(128);
    let session = Arc::as_ptr(&handles);
    let held = Arc::strong_count(&handles);
    let opened_files = files(&database);

    let me = thread::current().id();
    let mut records = Vec::<CommitRecord>::new();
    let mut work = handles.diagnostics().unwrap();
    let mut ready = harness.mount(1);
    let mut mount = root(&ready).to_path_buf();
    let mut drained = None;
    for (index, script) in ROUNDS.iter().enumerate() {
        let round = index + 1;
        if round == ROUNDS.len() {
            // The last Commit is on a second mount of the same open Store.
            quiet(&harness, ready.token);
            harness.unmount(&ready);
            drained = Some(round);
            ready = harness.mount(2);
            mount = root(&ready).to_path_buf();
        }
        let token = ready.token;
        passed(&bash_in(COMMAND, &mount, script), "changes by bash");
        let expected = native(&mount);
        quiet(&harness, token);
        let bound = harness.status(token);
        assert_eq!(bound.activity, Activity::Idle);
        // Nothing but a Commit uses the writer: mounting, serving reads and
        // writes, and walking the tree made no call on it.
        let between = watch.take();
        assert!(
            between.is_empty(),
            "{name}: writer calls outside a Commit: {between:?}"
        );
        let before = threads();

        let done = harness
            .try_commit(token)
            .unwrap_or_else(|failure| panic!("{name}: Commit {round}: {failure:?}"));
        let calls = watch.take();
        let after = threads();
        rig::receipt(&done, name);
        let commit = done.commit.as_ref().expect("the Commit receipt");
        let record = match &commit.history {
            CommitStagedOutcome::Committed(record) => record.clone(),
            other => panic!("{name}: Commit {round}: {other:?}"),
        };
        // The public receipt of this Commit's one producer handle.
        let storage = commit.storage;
        assert_eq!(storage.policy, 1, "{storage:?}");
        assert_eq!(storage.initial_reservations, 1, "{storage:?}");
        assert!(storage.publish >= 1, "{storage:?}");
        drop(done);
        // The shared writer saw one producer handle open, and one thread.
        let count = |port: Port| calls.iter().filter(|seen| seen.port == port).count() as u64;
        assert_eq!(
            count(Port::Policy),
            1,
            "{name}: producer handles: {calls:?}"
        );
        assert_eq!(count(Port::Publish), storage.publish, "{calls:?}");
        assert_eq!(count(Port::Reserve), storage.reserve, "{calls:?}");
        assert!(count(Port::Reserve) >= 1);
        assert!(
            calls.iter().all(|seen| seen.thread == me),
            "{name}: every writer call of the Commit is on the calling thread: {calls:?}"
        );
        // No thread was added for the Commit: the count at every writer call
        // and after the Commit is the count before it.
        let during = calls
            .iter()
            .map(|seen| seen.threads)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            (during.into_iter().collect::<Vec<_>>(), after),
            (vec![before], before),
            "{name}: threads during the Commit"
        );

        // The same open session: never dropped, sealed or reopened.
        assert!(std::ptr::eq(opened, Arc::as_ptr(&store)));
        assert!(std::ptr::eq(session, Arc::as_ptr(&handles)));
        assert_eq!(Arc::strong_count(&handles), held);
        assert_eq!(
            files(&database),
            opened_files,
            "the database and its log are the ones opened"
        );
        let now = handles.diagnostics().unwrap();
        assert!(
            now.write_commits > work.write_commits
                && now.transactions > work.transactions
                && now.statements > work.statements,
            "{name}: the writable session did this Commit's work: {work:?} -> {now:?}"
        );
        assert_eq!(now.rollbacks, work.rollbacks);
        let read_back = handles.profile();
        assert_eq!(
            (
                read_back.persistence,
                read_back.journal_mode.as_str(),
                read_back.synchronous
            ),
            (SqlitePersistenceProfile::Disposable, "wal", 0)
        );

        // The Commit is exact, with its predecessor as parent.
        assert_eq!(
            record.parent,
            records
                .last()
                .map(|previous| previous.id)
                .or(bound.binding.branch.head_commit)
        );
        assert_same(
            &expected,
            &published(&harness, &fresh, record.root),
            "the published root",
        );
        assert_same(&expected, &native(&mount), "the same mount after install");
        // Reading the published root back used the read sessions only.
        assert!(watch.take().is_empty());
        println!(
            "{name} COMMIT round={round} mount={} receipt(storage.policy={} initial_reservations={} reservation_refills={} reserve={} publish={}) writer_port(policy={} reserve={} publish={} calls={}) caller_threads=1 process_threads(before={before} during={before} after={after}) writer_session(write_commits {}->{} transactions {}->{}) same_store=true same_session=true wal_present=true",
            if drained.is_some() { 2 } else { 1 },
            storage.policy,
            storage.initial_reservations,
            storage.reservation_refills,
            storage.reserve,
            storage.publish,
            count(Port::Policy),
            count(Port::Reserve),
            count(Port::Publish),
            calls.len(),
            work.write_commits,
            now.write_commits,
            work.transactions,
            now.transactions
        );
        work = now;
        records.push(record);
    }
    assert_eq!(drained, Some(ROUNDS.len()));
    assert!(!hold.expired());
    quiet(&harness, ready.token);
    harness.unmount(&ready);
    println!(
        "{name} RESULT commits={} mounts=2 producers_per_commit=1 store=one_arc_never_dropped session=one_writable_never_sealed_or_reopened profile=Disposable/wal/synchronous=0 durable=NOT_RUN composing_calls={:?} LAYERFS_CONSTRUCTION_WORKERS=not_read_by_core/crates/*/src",
        records.len(),
        composing.iter().map(|seen| seen.port).collect::<Vec<_>>()
    );
    harness.stop();
    drop((store, watch, handles));
    fixture.cleanup();
}
