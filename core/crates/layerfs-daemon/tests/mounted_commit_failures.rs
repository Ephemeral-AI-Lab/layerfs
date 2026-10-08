//! Real kernel mounts: the failure matrix of the product control Commit
//! (R5-6) and what each failure does with the constructor's captured reader
//! and operation owner (R5-7). Every case changes the mount with externally
//! launched bash under the command identity, then attempts
//! `execute_control(Request::Commit)` once. No caller constructor is used.
//!
//! How each failure is staged, and at what scope:
//!
//! - F1: a real external python3 process holds the Store's SQLite writer when
//!   the Commit begins.
//! - F2: the same real held writer, taken around the real `stage_and_commit`
//!   by the external catalog wrapper of `support/history_boundary.rs`.
//! - F3: external catalog boundary scope; not a native SQLite I/O failure. The
//!   wrapper performs the real publication and reports an unknown outcome.
//! - F4: external catalog boundary scope. The wrapper performs the real
//!   publication and then reserves every free admission registration of the
//!   local owner through its public `submit_when_available`, submitting
//!   nothing, so the driver's one `InstallPrepared` is refused before effect.
//! - F5: a definite failure inside the producer: real Store Busy. A wrapper
//!   at the public storage port lets a real external python3 process hold the
//!   Store writer around the first real publication of the Save, which a
//!   change larger than one Save wave makes happen during construction. This
//!   is a Store refusal before effect, not a missing dependency.
//!
//! Product expectations are collected and reported after the mount has been
//! torn down, so a deviation still leaves its whole evidence and no mount.
//! Nothing here is timed.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/held_writer.rs"]
mod held_writer;
#[allow(dead_code)]
#[path = "support/history_boundary.rs"]
mod history_boundary;
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
use history_boundary::{Boundary, ObservedHistory};
use layerfs_bridge::control::{
    Activity, ControlCode, ControlRefusal, NativePhase, ReadyMount, Reply, Request,
    WorkspaceStatus, WorkspaceToken,
};
use layerfs_content::ObjectId;
use layerfs_daemon::{
    bootstrap::open_store,
    control::{Failure, Success},
    store::{
        CapturedConstruction, CommitError, CommitFailure, CommitPhase, ReadLimits, ReleasedOwner,
        Store, StoreReader,
    },
    Command, Completion, OwnerConfig, OwnerError, Response,
};
use layerfs_history::{
    CommitId, CommitRecord, CommitStagedOutcome, HistoryCatalog, HistoryError, WorkspaceId,
};
use layerfs_overlay::{Capture, CapturedReader, OperationOwner};
use layerfs_persistence::{Handles, SqlitePersistenceProfile};
use layerfs_storage::{port::PackPersistence, ReservationBlocks, Storage, StorageError};
use model::{assert_same, Flat, Model, FILE};
use mounted::{mount_entry, until, Harness, COMMAND};
use rig::{base_tree, bash_in, native, passed, root};
use std::{
    cell::Cell,
    fmt::Debug,
    fs,
    path::Path,
    process::Command as Process,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

/// External catalog boundary scope: test code at the public History port and
/// the owner's public admission. Not a product hook and not a native SQLite
/// I/O failure.
mod catalog_boundary {
    use layerfs_daemon::{Admission, Command, OwnerClient};
    use layerfs_history::*;
    use layerfs_overlay::Route;
    use layerfs_persistence::Handles;
    use std::sync::{Arc, Mutex};

    /// Admission registrations of the local owner, reserved and never polled:
    /// none of their commands is ever submitted. Dropping one frees its slot.
    pub struct Saturated {
        pub registrations: Vec<Admission>,
        /// The owner's refusal of the next registration.
        pub refusal: Option<String>,
    }
    /// Reserves registrations until the owner refuses one. The loop bound is
    /// above the owner's fixed registration capacity.
    pub fn saturate(client: &OwnerClient, route: Route) -> Saturated {
        let mut registrations = Vec::new();
        while registrations.len() < 4096 {
            match client.submit_when_available(Some(route), Command::Resources { global: false }) {
                Ok(waiting) => registrations.push(waiting),
                Err((cause, _)) => {
                    return Saturated {
                        registrations,
                        refusal: Some(format!("{cause:?}")),
                    }
                }
            }
        }
        Saturated {
            registrations,
            refusal: None,
        }
    }

    /// The real catalog. Once armed, the next `stage_and_commit` performs the
    /// real publication and, before returning its exact known outcome, takes
    /// every free admission registration of the armed owner.
    pub struct PublishedThenSaturated {
        pub handles: Arc<Handles>,
        pub armed: Mutex<Option<(OwnerClient, Route)>>,
        pub held: Mutex<Option<Saturated>>,
    }
    macro_rules! forward {
        ($($name:ident($($arg:ident: $ty:ty),*) -> $out:ty;)*) => {$(
            fn $name(&self, $($arg: $ty),*) -> $out { self.handles.history.$name($($arg),*) }
        )*};
    }
    impl HistoryCatalog for PublishedThenSaturated {
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
            let outcome = self.handles.history.stage_and_commit(request)?;
            let armed = self.armed.lock().unwrap().take();
            if let Some((client, route)) = armed {
                *self.held.lock().unwrap() = Some(saturate(&client, route));
            }
            Ok(outcome)
        }
    }
}
/// External storage boundary scope: test code at the public pack persistence
/// port, every call forwarded to the real provider. Not a product hook: the
/// refusal it stages is the real provider's own answer to a real held writer.
mod storage_boundary {
    use crate::held_writer::HeldWriter;
    use layerfs_content::ObjectId;
    use layerfs_storage::{
        location::{LocatedObject, SignatureRow},
        port::{
            AcquiredPackRead, PackPersistence, PackReadPlan, PersistedPack, PersistedPackRead,
            PersistenceError, Publication, Published, PublishedPack, Reserve, Reserved,
            ValueGroupQuery, ValueGroups,
        },
        StoragePolicy,
    };
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
    };

    /// The real writer. Once armed, a real external process holds the Store's
    /// SQLite writer around the next real `publish`, exactly once.
    pub struct BusyOnPublish {
        pub inner: Arc<dyn PackPersistence>,
        pub database: PathBuf,
        pub armed: AtomicBool,
        /// What the real provider answered under the held writer.
        pub observed: Mutex<Option<String>>,
    }
    type Answer<T> = Result<T, PersistenceError>;
    impl PackPersistence for BusyOnPublish {
        fn policy(&self) -> Answer<StoragePolicy> {
            self.inner.policy()
        }
        fn locate(&self, ids: &[ObjectId], out: &mut Vec<LocatedObject>) -> Answer<()> {
            self.inner.locate(ids, out)
        }
        fn read_packs(&self, ids: &[i64], out: &mut Vec<PersistedPack>) -> Answer<()> {
            self.inner.read_packs(ids, out)
        }
        fn read_pack_selection(
            &self,
            id: i64,
            plan: &mut dyn PackReadPlan,
        ) -> Answer<PersistedPackRead> {
            self.inner.read_pack_selection(id, plan)
        }
        fn read_scoped_pack(
            &self,
            id: i64,
            plan: &mut dyn PackReadPlan,
        ) -> Answer<AcquiredPackRead> {
            self.inner.read_scoped_pack(id, plan)
        }
        fn value_groups(&self, query: ValueGroupQuery<'_>) -> Answer<ValueGroups> {
            self.inner.value_groups(query)
        }
        fn signatures(&self, out: &mut Vec<SignatureRow>) -> Answer<()> {
            self.inner.signatures(out)
        }
        fn reserve(&self, request: Reserve) -> Answer<Reserved> {
            self.inner.reserve(request)
        }
        fn publication_pack_cost(&self, pack: &PublishedPack) -> Answer<(usize, u64)> {
            self.inner.publication_pack_cost(pack)
        }
        fn publish(&self, batch: &Publication) -> Answer<Published> {
            if !self.armed.swap(false, Ordering::SeqCst) {
                return self.inner.publish(batch);
            }
            let held = HeldWriter::acquire(&self.database);
            let result = self.inner.publish(batch);
            held.release();
            *self.observed.lock().unwrap() = Some(match &result {
                Ok(_) => "published".into(),
                Err(error) => format!("{error:?}"),
            });
            result
        }
    }
}
use catalog_boundary::PublishedThenSaturated;
use storage_boundary::BusyOnPublish;

/// Ordinary changes of several kinds, in the tree, inside `.git`, an ignored
/// directory, the cache and the build output. Run with the mount as the
/// working directory by a process nothing registered.
const CHANGES: &str = r#"
set -euo pipefail
umask 022
mkdir -p made/sub .git/objects/f0
printf 'created before the Commit\n' > made/new.txt
printf 'object made before the Commit' > .git/objects/f0/obj
printf 'appended line\n' >> README.md
printf 'OVERWRITTEN' | dd of=target/debug/app bs=1 seek=123456 conv=notrunc status=none
truncate -s 1000 .cache/tool/data.bin
chmod 0600 src/lib.rs
mv -T src/main.rs made/sub/main-moved.rs
ln -s ../new.txt made/sub/rel-link
ln made/new.txt made/new-alias.txt
rm -f spare/gone.txt ignored/secret.env
"#;
/// New incompressible bytes, more than one Save wave carries (4 MiB), so the
/// Save publishes its first packs while the producer is still constructing.
const BIG: &str = r#"
set -euo pipefail
umask 022
head -c 6291456 /dev/urandom > made/big.bin
"#;
const AFTER: &str = "made/after-failure.txt";
const FIRST: &[u8] = b"written after the failed Commit\n";
const SECOND: &[u8] = b"and after the refused unmount\n";

/// A bounded description of an original value for an evidence line.
fn brief(value: &impl Debug) -> String {
    let mut text = format!("{value:?}");
    let mut end = text.len().min(360);
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
            println!("F_DEVIATION {what}");
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
                "F_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}

enum Staging {
    /// The Store as `rig::Rig` opens it.
    Plain,
    /// `support/history_boundary.rs` around the real catalog.
    History(Boundary),
    /// `catalog_boundary::PublishedThenSaturated` around the real catalog.
    InstallRefused,
    /// `storage_boundary::BusyOnPublish` around the real writer.
    BusyPublication,
}
/// One installed Disposable Store, opened once and kept open, and one real
/// native serving assembly over it: `Harness::new`, as `rig::Rig` uses it.
struct Fx {
    fixture: installed::Fixture,
    store: Arc<Store>,
    harness: Harness,
    refusing: Option<Arc<PublishedThenSaturated>>,
    busy: Option<Arc<BusyOnPublish>>,
    fresh: Cell<u8>,
}
/// The same public pieces `open_store` composes, with the writer and History
/// ports chosen by the caller: one writable session, two read-only sessions,
/// one Store.
fn wrapped(
    fixture: &installed::Fixture,
    ports: impl FnOnce(Handles) -> (Arc<dyn PackPersistence>, Arc<dyn HistoryCatalog>),
) -> Arc<Store> {
    let config = || fixture.config.clone();
    let (writer, history) =
        ports(Handles::open_writable(config(), installed::BINDING, installed::CURSOR).unwrap());
    let readers = (0..2)
        .map(|_| {
            let read =
                Handles::open_read_only(config(), installed::BINDING, installed::CURSOR).unwrap();
            StoreReader::new(Storage::new(read.storage).unwrap(), Arc::new(read.history))
        })
        .collect();
    Arc::new(
        Store::new(
            writer,
            history,
            readers,
            2 * 1024 * 1024,
            ReservationBlocks::default(),
            ReadLimits::default(),
        )
        .unwrap(),
    )
}
impl Fx {
    fn new(label: &str, staging: Staging) -> Self {
        let fixture = installed::Fixture::built(label, |source| {
            base_tree(source);
            Model::native(source).1
        });
        assert!(
            matches!(
                fixture.config.sqlite_profile,
                SqlitePersistenceProfile::Disposable
            ),
            "the global Store profile is Disposable, selected explicitly"
        );
        let (mut refusing, mut busy) = (None, None);
        let store = match staging {
            Staging::Plain => open_store(
                fixture.config.clone(),
                installed::BINDING,
                installed::CURSOR,
                2,
                2 * 1024 * 1024,
                ReservationBlocks::default(),
            )
            .unwrap(),
            Staging::History(boundary) => wrapped(&fixture, |handles| {
                let handles = Arc::new(handles);
                let writer: Arc<dyn PackPersistence> = handles.storage.clone();
                let history: Arc<dyn HistoryCatalog> = Arc::new(ObservedHistory {
                    handles,
                    database: fixture.config.path.clone(),
                    boundary,
                    once: AtomicBool::new(true),
                });
                (writer, history)
            }),
            Staging::InstallRefused => wrapped(&fixture, |handles| {
                let handles = Arc::new(handles);
                let writer: Arc<dyn PackPersistence> = handles.storage.clone();
                let catalog = Arc::new(PublishedThenSaturated {
                    handles,
                    armed: Mutex::new(None),
                    held: Mutex::new(None),
                });
                refusing = Some(catalog.clone());
                let history: Arc<dyn HistoryCatalog> = catalog;
                (writer, history)
            }),
            Staging::BusyPublication => wrapped(&fixture, |handles| {
                let Handles {
                    storage, history, ..
                } = handles;
                let writer = Arc::new(BusyOnPublish {
                    inner: storage,
                    database: fixture.config.path.clone(),
                    armed: AtomicBool::new(false),
                    observed: Mutex::new(None),
                });
                busy = Some(writer.clone());
                let writer: Arc<dyn PackPersistence> = writer;
                let history: Arc<dyn HistoryCatalog> = Arc::new(history);
                (writer, history)
            }),
        };
        let harness = Harness::new(store.clone(), &fixture.directory, fixture.branch);
        Self {
            fixture,
            store,
            harness,
            refusing,
            busy,
            fresh: Cell::new(128),
        }
    }
    /// The Branch's current root, which must be `root`, walked completely
    /// through a fresh bind of a new Workspace that is closed again. Reads
    /// the Store only: no mount and no Workspace of the test is consulted.
    fn published(&self, root: ObjectId) -> Flat {
        let tag = self.fresh.get();
        self.fresh.set(tag.checked_add(1).expect("fresh bind tags"));
        let bound = self
            .harness
            .service
            .execute_control(&Request::Mount {
                workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
                branch: self.harness.branch,
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
        let operation = self.harness.service.operation(token).unwrap();
        let flat = model::canonical(operation.client(), root);
        assert!(operation.ports().failure().unwrap().is_none());
        drop(operation);
        let closed = self.harness.try_unmount(token).unwrap();
        assert_eq!(closed.reply, Reply::Unmounted(token));
        flat
    }
    /// `clean`: no mount and no custody remains, so the serving assembly is
    /// stopped with its checks. Otherwise the registry still owns original
    /// custody that nothing can settle; the kernel mount was already detached
    /// by the test and the assembly is dropped with that custody.
    fn finish(self, clean: bool) {
        let Self {
            fixture,
            store,
            harness,
            refusing,
            busy,
            ..
        } = self;
        drop((refusing, busy));
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
            owner.stop().unwrap();
        }
        println!(
            "F_RIG store_opened=1 sealed=0 reopened=0 profile=Disposable clean_stop={clean} handles_after_stop={}",
            Arc::strong_count(&store)
        );
        drop(store);
        fixture.cleanup();
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
/// A refusal for retained original custody, made before any effect.
fn refused(result: Result<Success, Failure>, what: &str) -> ControlRefusal {
    match result {
        Err(Failure::Custody(refusal)) => *refusal,
        Ok(done) => panic!("{what}: admitted past retained custody: {:?}", done.reply),
        Err(other) => panic!("{what}: {}", brief(&other)),
    }
}
/// The same mount after the failure: the view is what it was before the
/// Commit, and one more write by externally launched bash is served and read
/// back. Returns the tree with that write.
fn still_serving(mount: &Path, before: &Flat, what: &str) -> Flat {
    assert_same(
        before,
        &native(mount),
        &format!("{what}: the live view after the failed Commit"),
    );
    passed(
        &bash_in(
            COMMAND,
            mount,
            &format!(
                "set -euo pipefail; umask 022; printf 'written after the failed Commit\\n' > {AFTER}"
            ),
        ),
        what,
    );
    let after = native(mount);
    let made = after
        .get(AFTER.as_bytes())
        .unwrap_or_else(|| panic!("{what}: {AFTER} is not served"))
        .clone();
    assert_eq!(
        (made.kind, made.mode, made.links, made.payload.as_slice()),
        (FILE, 0o644, 1, FIRST),
        "{what}"
    );
    // Exactly the earlier tree, the new file and its parent's new time.
    let mut expected = before.clone();
    expected.insert(AFTER.as_bytes().to_vec(), made);
    expected.get_mut(b"made".as_slice()).unwrap().mtime = after[b"made".as_slice()].mtime;
    assert_same(
        &expected,
        &after,
        &format!("{what}: the earlier view plus one write"),
    );
    after
}

/// `Some` or `None` of an engine answer, for an evidence line.
fn answer<T>(value: &Option<T>) -> &'static str {
    if value.is_some() {
        "Some"
    } else {
        "None"
    }
}
/// The constructor's receipt, copied out of an outcome.
struct Owners {
    reader: Option<CapturedReader>,
    operation: Option<OperationOwner>,
    retained: bool,
    /// Releases known done, in attempt order.
    released: Vec<ReleasedOwner>,
    release_failure: Option<String>,
    release_error: Option<String>,
    custody: bool,
    work: bool,
    counters: bool,
}
impl Debug for Owners {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "reader={} operation={} retained={} released={:?} release_failure={:?} release_error={:?} custody={} work={} counters={}",
            answer(&self.reader),
            answer(&self.operation),
            self.retained,
            self.released,
            self.release_failure,
            self.release_error,
            self.custody,
            self.work,
            self.counters
        )
    }
}
impl Owners {
    fn of(namespace: &CapturedConstruction) -> Self {
        Self {
            reader: namespace.reader,
            operation: namespace.operation,
            retained: namespace.retained(),
            released: namespace.released.clone(),
            release_failure: namespace.release_failure.as_ref().map(brief),
            release_error: namespace.release_error.as_ref().map(brief),
            custody: namespace.custody.is_some(),
            work: namespace.work.is_some(),
            counters: namespace.counters.is_some(),
        }
    }
    /// R5-7: the reader released, then the operation owner; nothing kept.
    fn released(&self) -> bool {
        !self.retained
            && self.reader.is_none()
            && self.operation.is_none()
            && self.released == [ReleasedOwner::Reader, ReleasedOwner::Operation]
            && self.release_failure.is_none()
            && self.release_error.is_none()
            && !self.custody
    }
    /// R5-7: nothing released; the failure names the reader and the owner.
    fn kept(&self) -> bool {
        self.retained
            && self.reader.is_some()
            && self.operation.is_some()
            && self.released.is_empty()
            && self.release_failure.is_none()
            && self.release_error.is_none()
    }
}
fn done(completion: &Completion) -> bool {
    matches!(completion.result(), Ok(Response::Done))
}
/// What one failed product Commit knew, copied out of the original failure.
#[derive(Debug)]
struct Left {
    phase: CommitPhase,
    error: String,
    /// The error is exactly the original variant the case staged.
    original: bool,
    published: Option<CommitStagedOutcome>,
    settled: bool,
    capture: Capture,
    candidate: Option<ObjectId>,
    /// The definite-failure resolution completion answered Done.
    local: Option<bool>,
    local_error: Option<String>,
    namespace: Option<Owners>,
}
impl Left {
    /// The product constructor's request number: the capture's generation.
    fn request(&self) -> u64 {
        u64::try_from(self.capture.generation.number()).unwrap()
    }
}

/// One mounted Workspace changed by external bash, about to be committed.
struct Case {
    name: &'static str,
    /// First field: dropped first, also when the test unwinds.
    mounted: Mounted,
    fx: Fx,
    ready: ReadyMount,
    /// The tree the mount showed before the Commit, by ordinary syscalls.
    before: Flat,
    /// The registry's view before the Commit.
    bound: WorkspaceStatus,
    checks: Checks,
}
impl Case {
    /// `scripts` run in order, each by its own externally launched bash.
    fn start(name: &'static str, how: &str, staging: Staging, scripts: &[&str]) -> Self {
        let fx = Fx::new(&name.to_lowercase(), staging);
        let ready = fx.harness.mount(1);
        let mounted = Mounted(ready.directory.clone());
        let mount = root(&ready);
        for script in scripts {
            passed(
                &bash_in(COMMAND, mount, script),
                "changes by externally launched bash",
            );
        }
        let before = native(mount);
        let new = &before[b"made/new.txt".as_slice()];
        assert_eq!(
            (new.payload.as_slice(), new.links),
            (b"created before the Commit\n".as_slice(), 2)
        );
        assert_eq!(
            before[b"README.md".as_slice()].payload,
            b"# base\nappended line\n"
        );
        assert_eq!(
            before[b"made/sub/main-moved.rs".as_slice()].payload,
            b"fn main() {}\n"
        );
        assert!(!before.contains_key(b"src/main.rs".as_slice()));
        quiet(&fx.harness, ready.token);
        let bound = fx.harness.status(ready.token);
        assert_eq!(bound.activity, Activity::Idle);
        println!(
            "{name} STAGED how={how} route=execute_control(Request::Commit) mount={} changes_by=bash uid={COMMAND} gid={COMMAND} paths={} lifecycle_slots={} ordinary_slots={} bound_head={:?} epoch={}",
            ready.directory,
            before.len(),
            OwnerConfig::default().lifecycle_jobs_per_namespace,
            OwnerConfig::default().jobs_per_namespace,
            bound.binding.branch.head_commit,
            bound.epoch
        );
        Self {
            name,
            mounted,
            fx,
            ready,
            before,
            bound,
            checks: Checks::default(),
        }
    }
    fn token(&self) -> WorkspaceToken {
        self.ready.token
    }
    /// The registry and engine observation of a quiet connection.
    fn status(&self) -> WorkspaceStatus {
        quiet(&self.fx.harness, self.token());
        self.fx.harness.status(self.token())
    }
    /// One product control Commit, attempted once on this thread.
    fn commit(&self) -> Result<Success, Failure> {
        quiet(&self.fx.harness, self.token());
        self.fx.harness.try_commit(self.token())
    }
    /// The original Commit failure and what it knew.
    fn failure(
        &self,
        result: Result<Success, Failure>,
        original: impl FnOnce(&CommitError) -> bool,
    ) -> (Left, Box<CommitFailure>) {
        let name = self.name;
        let failure = match result {
            Err(Failure::Commit(failure)) => failure,
            Ok(done) => panic!("{name}: the Commit succeeded: {:?}", done.reply),
            Err(other) => panic!("{name}: not a Commit failure: {}", brief(&other)),
        };
        let left = Left {
            phase: failure.phase,
            error: brief(&failure.error),
            original: original(&failure.error),
            published: failure.published.clone(),
            settled: failure.locally_settled,
            capture: failure
                .capture
                .unwrap_or_else(|| panic!("{name}: no capture: {}", brief(&failure))),
            candidate: failure.intent.as_ref().map(|intent| intent.candidate_root),
            local: failure.local.as_ref().map(done),
            local_error: failure.local_error.as_ref().map(brief),
            namespace: failure.namespace.as_ref().map(Owners::of),
        };
        println!(
            "{name} FAILURE phase={:?} error={} original_variant={} published={} locally_settled={} local_done={:?} local_error={:?} request={} candidate={:?} namespace={:?}",
            left.phase,
            left.error,
            left.original,
            left.published.as_ref().map_or("None".into(), brief),
            left.settled,
            left.local,
            left.local_error,
            left.request(),
            left.candidate,
            left.namespace
        );
        (left, failure)
    }
    /// What the engine itself holds for one request: the two public
    /// `Retained*` owner jobs, each asked once on a quiet connection.
    fn engine(&self, request: u64) -> (Option<CapturedReader>, Option<OperationOwner>) {
        quiet(&self.fx.harness, self.token());
        let operation = self.fx.harness.service.operation(self.token()).unwrap();
        let route = operation.workspace().route();
        let ask = |command: Command| {
            operation
                .overlay()
                .try_submit(Some(route), command)
                .unwrap()
                .wait()
                .unwrap()
        };
        let reader = match ask(Command::RetainedCapturedReader { request }).result() {
            Ok(Response::CapturedReader(kept)) => *kept,
            other => panic!("{other:?}"),
        };
        let owner = match ask(Command::RetainedOperation { request }).result() {
            Ok(Response::Operation(kept)) => *kept,
            other => panic!("{other:?}"),
        };
        (reader, owner)
    }
    /// A later explicit Commit, a new operation: it must publish exactly the
    /// tree the mount shows, read back from a fresh bind and the same mount.
    fn later_exact(&mut self, mount: &Path, parent: Option<CommitId>) -> Option<CommitRecord> {
        let name = self.name;
        let expected = native(mount);
        let done = match self.commit() {
            Ok(done) => done,
            Err(failure) => {
                expect!(
                    self.checks,
                    false,
                    "{name}: a later explicit Commit failed: {}",
                    brief(&failure)
                );
                return None;
            }
        };
        let record = match &done.reply {
            Reply::Committed(CommitStagedOutcome::Committed(record)) => record.clone(),
            other => {
                expect!(
                    self.checks,
                    false,
                    "{name}: a later explicit Commit replied {other:?}"
                );
                return None;
            }
        };
        let commit = done.commit.as_ref().expect("Commit receipt");
        let request = u64::try_from(commit.capture.generation.number()).unwrap();
        let owners = commit.namespace.as_ref().map(Owners::of);
        drop(done);
        expect!(
            self.checks,
            record.parent == parent,
            "{name}: the later Commit's parent is {:?}, the bound head is {parent:?}",
            record.parent
        );
        expect!(
            self.checks,
            owners.as_ref().is_some_and(Owners::released),
            "{name}: the later Commit releases its reader, then its owner: {owners:?}"
        );
        let engine = self.engine(request);
        expect!(
            self.checks,
            engine == (None, None),
            "{name}: the engine keeps owners of the later Commit: {engine:?}"
        );
        let published = self.fx.published(record.root);
        assert_same(
            &expected,
            &published,
            &format!("{name}: the later Commit's root, fresh bind"),
        );
        assert_same(
            &expected,
            &native(mount),
            &format!("{name}: the same mount after the later install"),
        );
        let status = self.status();
        expect!(
            self.checks,
            status.activity == Activity::Idle
                && status.published.is_none()
                && status.binding.effective_root == record.root
                && status.binding.branch.head_commit == Some(record.id),
            "{name}: registry after the later Commit: activity={:?} published={:?} root={:?}",
            status.activity,
            status.published,
            status.binding.effective_root
        );
        println!(
            "{name} LATER_COMMIT outcome=Committed parent={:?} request={request} paths={} oracles=published_fresh_bind,same_mount owners={owners:?} RetainedCapturedReader={} RetainedOperation={} activity={:?}",
            record.parent,
            published.len(),
            answer(&engine.0),
            answer(&engine.1),
            status.activity
        );
        Some(record)
    }
    /// Normal terminal unmount; true when the product unmounted.
    fn unmount(&mut self) -> bool {
        let name = self.name;
        let token = self.token();
        let directory = self.ready.directory.clone();
        quiet(&self.fx.harness, token);
        match self.fx.harness.try_unmount(token) {
            Ok(done) => {
                let receipt = format!("{:?}", done.native);
                let gone = mount_entry(&directory).is_none() && !Path::new(&directory).exists();
                expect!(
                    self.checks,
                    done.reply == Reply::Unmounted(token)
                        && done.earlier.len() == 1
                        && done.completion.is_some()
                        && receipt.contains("Drained")
                        && gone,
                    "{name}: normal unmount: reply={:?} earlier={} mount_gone={gone} receipt={}",
                    done.reply,
                    done.earlier.len(),
                    brief(&receipt)
                );
                println!(
                    "{name} UNMOUNT normal=Unmounted drained={} mount_table_entry_gone={gone}",
                    receipt.contains("Drained")
                );
                true
            }
            Err(failure) => {
                expect!(
                    self.checks,
                    false,
                    "{name}: normal unmount after a settled failure: {}",
                    brief(&failure)
                );
                false
            }
        }
    }
    /// A definite failure the driver settled locally: registry Idle, nothing
    /// in the engine, the mount serving, a later Commit exact, unmount normal.
    /// `constructed` says whether the product constructor ran.
    fn definite_tail(mut self, left: Left, constructed: bool) {
        let name = self.name;
        let request = left.request();
        expect!(
            self.checks,
            left.original,
            "{name}: the error is not the original staged cause: {}",
            left.error
        );
        expect!(
            self.checks,
            left.published.is_none() && left.settled && left.local == Some(true),
            "{name}: a definite nonpublication is resolved locally: published={:?} settled={} local={:?} local_error={:?}",
            left.published,
            left.settled,
            left.local,
            left.local_error
        );
        if constructed {
            expect!(
                self.checks,
                left.namespace.as_ref().is_some_and(Owners::released),
                "{name}: a definite, locally settled failure releases the reader, then the operation owner, by two completions: {:?}",
                left.namespace
            );
        } else {
            expect!(
                self.checks,
                left.namespace.is_none(),
                "{name}: the constructor ran: {:?}",
                left.namespace
            );
        }
        let status = self.status();
        expect!(
            self.checks,
            status.activity == Activity::Idle
                && status.published.is_none()
                && status.binding == self.bound.binding,
            "{name}: registry after a settled definite failure: activity={:?} published={:?}",
            status.activity,
            status.published
        );
        let engine = self.engine(request);
        expect!(
            self.checks,
            engine == (None, None),
            "{name}: the engine still holds owners of request {request}: RetainedCapturedReader={:?} RetainedOperation={:?}",
            engine.0,
            engine.1
        );
        println!(
            "{name} REGISTRY activity={:?} published={:?} epoch={} local_captured={:?} | ENGINE request={request} RetainedCapturedReader={} RetainedOperation={}",
            status.activity,
            status.published,
            status.epoch,
            status.local.as_ref().map(|local| local.captured),
            answer(&engine.0),
            answer(&engine.1)
        );
        let mount = root(&self.ready).to_path_buf();
        let after = still_serving(&mount, &self.before, name);
        println!(
            "{name} MOUNT same_view_after_failure=true new_write_served=true paths={}",
            after.len()
        );
        let parent = self.bound.binding.branch.head_commit;
        let later = self.later_exact(&mount, parent);
        let leftover = self.engine(request);
        expect!(
            self.checks,
            leftover == (None, None),
            "{name}: after the later Commit the engine still holds owners of the failed request {request}: {leftover:?}"
        );
        let unmounted = self.unmount();
        let directory = self.ready.directory.clone();
        if mount_entry(&directory).is_some() {
            let done = Process::new("umount").arg(&directory).output().unwrap();
            println!(
                "{name} TEARDOWN by_test=plain umount(8) status={:?}",
                done.status
            );
        }
        println!(
            "{name} RESULT later_commit={} unmount={} deviations={}",
            if later.is_some() {
                "Committed,exact"
            } else {
                "FAILED"
            },
            if unmounted { "Unmounted" } else { "REFUSED" },
            self.checks.0.len()
        );
        let Self {
            fx,
            mounted,
            checks,
            ..
        } = self;
        fx.finish(unmounted);
        drop(mounted);
        checks.done(name);
    }
    /// A failure whose custody stays: the failure and the engine name the
    /// same reader and owner, later Commit and normal unmount are refused
    /// Unknown before any effect, the mount keeps serving, and an independent
    /// observation of the publication settles nothing. The test then detaches
    /// the kernel mount itself.
    fn retained_tail(mut self, left: Left, failure: Box<CommitFailure>, activity: Activity) {
        let name = self.name;
        let token = self.token();
        let request = left.request();
        let directory = self.ready.directory.clone();
        let mount = root(&self.ready).to_path_buf();
        expect!(
            self.checks,
            left.original,
            "{name}: the error is not the original staged cause: {}",
            left.error
        );
        expect!(
            self.checks,
            !left.settled && left.local.is_none() && left.local_error.is_none(),
            "{name}: nothing local is resolved: settled={} local={:?} local_error={:?}",
            left.settled,
            left.local,
            left.local_error
        );
        expect!(
            self.checks,
            left.namespace.as_ref().is_some_and(Owners::kept),
            "{name}: an unsettled failure keeps the reader and the operation owner: {:?}",
            left.namespace
        );
        expect!(
            self.checks,
            failure
                .namespace
                .as_ref()
                .is_some_and(CapturedConstruction::retained),
            "{name}: the original failure carries the retained owners"
        );
        let kept = left
            .namespace
            .as_ref()
            .map(|owners| (owners.reader, owners.operation));
        let engine = self.engine(request);
        expect!(
            self.checks,
            engine.0.is_some() && engine.1.is_some() && Some(engine) == kept,
            "{name}: the engine answers the failure's reader and owner for request {request}: engine={engine:?} failure={kept:?}"
        );
        let status = self.status();
        let captured = status.local.as_ref().and_then(|local| local.captured);
        expect!(
            self.checks,
            status.activity == activity
                && status.published == left.published
                && status.binding == self.bound.binding
                && captured == Some(left.capture.generation.number()),
            "{name}: registry after the failure: activity={:?} published={:?} captured={captured:?}",
            status.activity,
            status.published
        );
        println!(
            "{name} REGISTRY activity={:?} published={} epoch={} local_captured={captured:?} | ENGINE request={request} RetainedCapturedReader={} RetainedOperation={} engine_equals_failure={}",
            status.activity,
            status.published.as_ref().map_or("None".into(), brief),
            status.epoch,
            answer(&engine.0),
            answer(&engine.1),
            Some(engine) == kept
        );

        let after = still_serving(&mount, &self.before, name);
        println!(
            "{name} MOUNT same_view_after_failure=true new_write_served=true paths={}",
            after.len()
        );

        // A later Commit is refused before any effect: no new capture, no
        // registry change, the same owners in the engine.
        let same = |a: &WorkspaceStatus, b: &WorkspaceStatus| {
            a.epoch == b.epoch
                && a.activity == b.activity
                && a.published == b.published
                && a.binding == b.binding
                && a.local == b.local
        };
        let before_commit = self.status();
        let commit = refused(self.fx.harness.try_commit(token), "a later Commit");
        let after_commit = self.status();
        expect!(
            self.checks,
            commit.code == ControlCode::Unknown && commit.published == left.published,
            "{name}: a later Commit is refused Unknown with the known publication: {commit:?}"
        );
        let engine_after_commit = self.engine(request);
        expect!(
            self.checks,
            same(&before_commit, &after_commit) && engine_after_commit == engine,
            "{name}: the refused Commit changed the registry or the engine: epoch {} -> {}, local {:?} -> {:?}",
            before_commit.epoch,
            after_commit.epoch,
            before_commit.local,
            after_commit.local
        );
        // Normal unmount is refused the same way and detaches nothing.
        let unmount = refused(self.fx.harness.try_unmount(token), "normal unmount");
        let after_unmount = self.status();
        let phase = after_unmount.native.as_ref().map(|native| native.phase);
        expect!(
            self.checks,
            unmount.code == ControlCode::Unknown && unmount.published == left.published,
            "{name}: normal unmount is refused Unknown: {unmount:?}"
        );
        expect!(
            self.checks,
            same(&before_commit, &after_unmount)
                && phase == Some(NativePhase::Ready)
                && mount_entry(&directory).is_some(),
            "{name}: the refused unmount had an effect: phase={phase:?} mounted={}",
            mount_entry(&directory).is_some()
        );
        println!(
            "{name} REFUSALS later_commit=code:{:?},phase:{},published:{} unmount=code:{:?},phase:{} epoch_unchanged={} captured_unchanged={} native_phase={phase:?} mount_table_entry=present",
            commit.code,
            commit.phase,
            commit.published.is_some(),
            unmount.code,
            unmount.phase,
            before_commit.epoch == after_unmount.epoch,
            before_commit.local == after_unmount.local
        );
        // The mount goes on serving reads and writes under retained custody.
        passed(
            &bash_in(
                COMMAND,
                &mount,
                &format!(
                    "set -euo pipefail; printf 'and after the refused unmount\\n' >> {AFTER}; cat README.md target/debug/app > /dev/null"
                ),
            ),
            "write after the refused unmount",
        );
        assert_eq!(
            fs::read(mount.join(AFTER)).unwrap(),
            [FIRST, SECOND].concat()
        );
        println!("{name} MOUNT serves_reads_and_writes_under_retained_custody=true");

        // Two observations that do not go through this Workspace: a separate
        // read-only session of the Store, and a fresh bind of the Branch. The
        // candidate is published, and its tree is the captured frontier.
        let candidate = left.candidate.expect("the staged candidate");
        let observer = Handles::open_read_only(
            self.fx.fixture.config.clone(),
            installed::BINDING,
            installed::CURSOR,
        )
        .unwrap();
        let observed = observer
            .history
            .branch_snapshot(self.fx.harness.branch)
            .unwrap()
            .unwrap();
        drop(observer);
        expect!(
            self.checks,
            observed.effective_root == candidate
                && observed.branch.head_commit.is_some()
                && candidate != self.bound.binding.effective_root,
            "{name}: an independent session reads the candidate as the Branch root: observed={:?} candidate={candidate:?}",
            observed.effective_root
        );
        let published = self.fx.published(candidate);
        assert_same(
            &self.before,
            &published,
            &format!("{name}: the published candidate, fresh bind"),
        );
        // Neither observation settles the original failure.
        let observed_status = self.status();
        let observed_engine = self.engine(request);
        expect!(
            self.checks,
            observed_status.activity == activity
                && observed_status.published == left.published
                && observed_status.binding == self.bound.binding
                && observed_status.epoch == before_commit.epoch
                && observed_engine == engine,
            "{name}: an observation settled the original failure: activity={:?} published={:?} epoch={}",
            observed_status.activity,
            observed_status.published,
            observed_status.epoch
        );
        println!(
            "{name} OBSERVED independent_session_root=candidate head={:?} fresh_bind_root=candidate fresh_bind_paths={} original_failure_settled_by_observation=false activity={:?}",
            observed.branch.head_commit,
            published.len(),
            observed_status.activity
        );

        // Teardown by the test. The product refuses normal unmount for this
        // custody and has no forced unmount: one plain umount(8), launched
        // and reaped here, detaches the kernel mount. The registry entry and
        // its session are then dropped with the serving assembly.
        quiet(&self.fx.harness, token);
        let detached = Process::new("umount").arg(&directory).output().unwrap();
        assert!(detached.status.success(), "{detached:?}");
        assert!(mount_entry(&directory).is_none());
        let deadline = Instant::now() + Duration::from_secs(3);
        let (loops, phase) = loop {
            let native = self.fx.harness.status(token).native.unwrap();
            let loops = native
                .work
                .map_or((0, 0), |work| (work.loops_exited, work.loops_configured));
            if (loops.0 == loops.1 && loops.1 != 0) || Instant::now() >= deadline {
                break (loops, native.phase);
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        println!(
            "{name} TEARDOWN by_test=plain umount(8) of {directory} status={:?} mount_table_entry=gone receive_loops_exited={}/{} registry_phase_after={phase:?} product_unmount=refused forced_unmount=none deviations={}",
            detached.status,
            loops.0,
            loops.1,
            self.checks.0.len()
        );
        let Self {
            fx,
            mounted,
            checks,
            ..
        } = self;
        drop(failure);
        fx.finish(false);
        drop(mounted);
        checks.done(name);
    }
}

/// F1. Real Store Busy before the Save: a real external process holds the
/// Store writer when the Commit begins.
#[test]
fn f1_store_busy_before_save_is_settled_and_a_later_commit_is_exact() {
    let mut case = Case::start(
        "F1",
        "external python3 process holds the Store SQLite writer (BEGIN IMMEDIATE) when Commit begins",
        Staging::Plain,
        &[CHANGES],
    );
    let held = held_writer::HeldWriter::acquire(&case.fx.fixture.config.path);
    let result = case.commit();
    held.release();
    let (left, failure) = case.failure(result, |error| {
        matches!(error, CommitError::Storage(StorageError::Busy))
    });
    drop(failure);
    expect!(
        case.checks,
        left.phase == CommitPhase::Begin,
        "F1: phase {:?}",
        left.phase
    );
    case.definite_tail(left, false);
}

/// F2. Real Store Busy at publication, after the Save finished: definite, so
/// the driver resolves it and the constructor releases both owners.
#[test]
fn f2_store_busy_at_publication_is_settled_and_releases_both_owners() {
    let mut case = Case::start(
        "F2",
        "external python3 process holds the Store SQLite writer around the real stage_and_commit (support/history_boundary.rs Boundary::HeldWriter)",
        Staging::History(Boundary::HeldWriter),
        &[CHANGES],
    );
    let result = case.commit();
    let (left, failure) = case.failure(result, |error| {
        matches!(error, CommitError::History(HistoryError::Busy))
    });
    drop(failure);
    expect!(
        case.checks,
        left.phase == CommitPhase::Publish
            && left
                .namespace
                .as_ref()
                .is_some_and(|owners| owners.work && owners.counters),
        "F2: phase {:?}; the constructor ran to its root: {:?}",
        left.phase,
        left.namespace
    );
    case.definite_tail(left, true);
}

/// F3. Lost History acknowledgement. External catalog boundary scope; not a
/// native SQLite I/O failure: the wrapper performs the real publication and
/// then reports an unknown outcome.
#[test]
fn f3_unknown_history_outcome_keeps_custody_and_the_mount_still_serves() {
    let mut case = Case::start(
        "F3",
        "external catalog boundary scope; not a native SQLite I/O failure (support/history_boundary.rs Boundary::LostAcknowledgement: real publication, then UnknownOutcome)",
        Staging::History(Boundary::LostAcknowledgement),
        &[CHANGES],
    );
    let result = case.commit();
    let (left, failure) = case.failure(result, |error| {
        matches!(error, CommitError::History(HistoryError::UnknownOutcome))
    });
    expect!(
        case.checks,
        left.phase == CommitPhase::Publish && left.published.is_none(),
        "F3: phase {:?} published {:?}",
        left.phase,
        left.published
    );
    case.retained_tail(left, failure, Activity::Uncertain);
}

/// F4. Known publication, local install not attempted. External catalog
/// boundary scope: after the real publication the wrapper reserves every free
/// admission registration of the local owner and submits nothing, so the
/// driver's one `InstallPrepared` is refused before effect.
#[test]
fn f4_known_publication_with_an_unattempted_install_keeps_custody() {
    let mut case = Case::start(
        "F4",
        "external catalog boundary scope: real publication, then every free admission registration of the local owner is reserved through public submit_when_available (nothing submitted) before stage_and_commit returns",
        Staging::InstallRefused,
        &[CHANGES],
    );
    let refusing = case.fx.refusing.clone().expect("catalog boundary");
    let operation = case.fx.harness.service.operation(case.token()).unwrap();
    let route = operation.workspace().route();
    drop(operation);
    *refusing.armed.lock().unwrap() = Some((case.fx.harness.owner.client(), route));
    let result = case.commit();
    // The external effect ends here: every registration is dropped unpolled.
    let held = refusing
        .held
        .lock()
        .unwrap()
        .take()
        .expect("the boundary ran");
    let (count, refusal) = (held.registrations.len(), held.refusal.clone());
    drop(held);
    println!("F4 BOUNDARY reserved_registrations={count} submitted=0 next_registration_refused={refusal:?}");
    assert!(
        refusal.as_deref() == Some("AdmissionFull") && count > 0,
        "staging: {count} reserved, refusal {refusal:?}"
    );
    let (left, failure) = case.failure(result, |error| {
        matches!(
            error,
            CommitError::Owner(OwnerError::Unattempted { cause, command })
                if matches!(**cause, OwnerError::AdmissionFull)
                    && matches!(**command, Command::InstallPrepared { .. })
        )
    });
    let published = match &left.published {
        Some(CommitStagedOutcome::Committed(record)) => {
            Some(record.root) == left.candidate
                && record.parent == case.bound.binding.branch.head_commit
        }
        _ => false,
    };
    expect!(
        case.checks,
        left.phase == CommitPhase::Install && published,
        "F4: phase {:?} published {:?} candidate {:?}",
        left.phase,
        left.published,
        left.candidate
    );
    case.retained_tail(left, failure, Activity::LocalFailure);
}

/// F5. A definite failure inside the producer: real Store Busy. A change
/// larger than one Save wave makes the Save publish packs during
/// construction; the storage boundary lets a real external process hold the
/// Store writer around that first real publication. Not a missing dependency.
#[test]
fn f5_store_busy_inside_the_producer_is_settled_and_releases_both_owners() {
    let mut case = Case::start(
        "F5",
        "external python3 process holds the Store SQLite writer around the Save's first real publish, which a 6 MiB new file makes happen during construction (storage boundary, every call forwarded)",
        Staging::BusyPublication,
        &[CHANGES, BIG],
    );
    let busy = case.fx.busy.clone().expect("storage boundary");
    busy.armed.store(true, Ordering::SeqCst);
    let result = case.commit();
    let observed = busy.observed.lock().unwrap().take();
    println!("F5 BOUNDARY real_publish_under_held_writer={observed:?}");
    assert_eq!(
        observed.as_deref(),
        Some("Busy"),
        "staging: the real provider refuses Busy before effect"
    );
    let (left, failure) = case.failure(result, |error| {
        matches!(
            error,
            CommitError::Construction {
                storage: StorageError::Busy,
                ..
            }
        )
    });
    drop(failure);
    expect!(
        case.checks,
        left.phase == CommitPhase::Construct
            && left
                .namespace
                .as_ref()
                .is_some_and(|owners| owners.work && !owners.counters),
        "F5: phase {:?}; the producer ran and built no root: {:?}",
        left.phase,
        left.namespace
    );
    case.definite_tail(left, true);
}
