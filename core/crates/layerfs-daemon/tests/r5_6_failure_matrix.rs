//! Real kernel mounts: the halves of the product control Commit's failure
//! matrix (row R5-6) and of its release rule (row R5-7) that
//! `mounted_commit_failures.rs` names as not staged. Every case changes the
//! mount with externally launched bash under the command identity and then
//! attempts one product control Commit, served once over a real authenticated
//! control connection so that the original failure and the reply that was
//! sent are both read. No caller constructor is used and nothing is retried.
//!
//! How each failure is staged, and at what scope:
//!
//! - R5-6a, a missing dependency: real Store damage. An external python3
//!   SQLite process removes the one `object_location` row of an untouched
//!   chunk of a large file. The Commit then overwrites a few bytes of the
//!   same file several chunks away, so the rewritten extent page still
//!   references the removed chunk and the Commit never reads it. The row is
//!   put back by the same kind of process.
//! - R5-6b, an attempted local install that fails after a known publication,
//!   and R5-6c, an unknown History outcome with nothing published: a real
//!   operating-system limit. The Commit thread is held at the existing
//!   history gate (`support/history_gate.rs`); the test sets this process's
//!   file-size limit to zero with `SIGXFSZ` ignored and releases the gate, so
//!   the next write to a regular file is refused by the kernel. The failure is
//!   the engine's or the provider's own; no wrapper authors an answer.
//! - R5-7, a release refused after a settled failure: external owner-client
//!   scope. While the Commit thread is held before its publication, the test
//!   releases the Commit's captured reader itself through the public owner
//!   client, then lets a real external process hold the Store writer, so the
//!   real catalog answers Busy (definite), the driver settles, and the
//!   product's own release of that reader is performed and refused. The
//!   foreign release is a job no product path issues.
//!
//! The file-size limit is process-wide: while it is zero nothing in this
//! process may write a regular file. Every test here therefore takes one lock
//! first, so no two of them overlap when the binary runs its tests in
//! parallel, and nothing is printed inside the window.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/held_writer.rs"]
mod held_writer;
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
use history_gate::{Hold, Point};
use layerfs_bridge::control::{
    Activity, Answer, Call, ControlCode, ControlRefusal, NativePhase, ReadyMount, Reply, Request,
    WorkspaceStatus, WorkspaceToken,
};
use layerfs_content::{
    file::mapping::{decode_file_state, decode_node_with_context, ExtentNode},
    filesystem::{FilesystemRead, FilesystemRootId, LogicalPath},
    AuthenticatedObjects, ContentError, ObjectId,
};
use layerfs_daemon::{
    bootstrap::open_store,
    control::{Failure, Success},
    store::{
        CapturedConstruction, CommitError, CommitFailure, CommitPhase, ReadLimits, ReleasedOwner,
        Store, StoreReader,
    },
    Command, Completion, OwnerError, Response,
};
use layerfs_history::{
    CommitId, CommitStagedOutcome, HistoryCatalog, HistoryError, StageRecord, WorkspaceId,
};
use layerfs_overlay::{CapturedReader, OperationOwner, OverlayError, Route};
use layerfs_persistence::{Handles, SqlitePersistenceProfile};
use layerfs_storage::{port::PackPersistence, ReservationBlocks, Storage, StorageError};
use model::{assert_same, Flat, Model, FILE};
use mounted::{mount_entry, until, Harness, COMMAND};
use nix::libc;
use rig::{base_tree, bash_in, native, passed, root};
use std::{
    cell::Cell,
    fmt::Debug,
    fs,
    io::Read,
    os::unix::fs::MetadataExt,
    path::Path,
    process::{Command as Process, Stdio},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

/// One test of this binary at a time: see the file-size limit above.
static SERIAL: Mutex<()> = Mutex::new(());
fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|error| error.into_inner())
}
/// How long the test waits for the Commit thread to reach the gate.
const WAIT: Duration = Duration::from_secs(5);
/// How long the Commit thread stays held if nothing releases it.
const LIMIT: Duration = Duration::from_secs(20);

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
const AFTER: &str = "made-after-failure.txt";

/// A bounded description of an original value for an evidence line.
fn brief(value: &impl Debug) -> String {
    let mut text = format!("{value:?}");
    let mut end = text.len().min(420);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
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
                "R5_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}

/// One installed Disposable Store, opened once and kept open, and one real
/// native serving assembly over it.
struct Fx {
    fixture: installed::Fixture,
    store: Arc<Store>,
    harness: Harness,
    /// The writable session, when this file composed the Store itself.
    writer: Option<Arc<Handles>>,
    fresh: Cell<u8>,
}
impl Fx {
    fn fixture(label: &str) -> installed::Fixture {
        let fixture = installed::Fixture::built(label, |source| {
            base_tree(source);
            fs::create_dir(source.join("big")).unwrap();
            fs::write(source.join(NOISE), noise()).unwrap();
            rig::stamp(&source.join(NOISE), 1_500_000_000, 1);
            rig::stamp(&source.join("big"), 1_500_000_001, 2);
            Model::native(source).1
        });
        assert!(
            matches!(
                fixture.config.sqlite_profile,
                SqlitePersistenceProfile::Disposable
            ),
            "the global Store profile is Disposable, selected explicitly"
        );
        fixture
    }
    /// The Store as `rig::Rig` opens it.
    fn plain(label: &str) -> Self {
        let fixture = Self::fixture(label);
        let store = open_store(
            fixture.config.clone(),
            installed::BINDING,
            installed::CURSOR,
            2,
            2 * 1024 * 1024,
            ReservationBlocks::default(),
        )
        .unwrap();
        let harness = Harness::new(store.clone(), &fixture.directory, fixture.branch);
        Self {
            fixture,
            store,
            harness,
            writer: None,
            fresh: Cell::new(128),
        }
    }
    /// The same public pieces `open_store` composes, with the real catalog
    /// behind the existing history gate: its one publication is held on the
    /// chosen side of the real `stage_and_commit`, every call forwarded.
    fn gated(label: &str, point: Point) -> (Self, Hold) {
        let fixture = Self::fixture(label);
        let config = || fixture.config.clone();
        let handles = Arc::new(
            Handles::open_writable(config(), installed::BINDING, installed::CURSOR).unwrap(),
        );
        let writer: Arc<dyn PackPersistence> = handles.storage.clone();
        let (catalog, hold) = history_gate::gated(handles.clone(), point, LIMIT);
        let history: Arc<dyn HistoryCatalog> = Arc::new(catalog);
        let readers = (0..2)
            .map(|_| {
                let read = Handles::open_read_only(config(), installed::BINDING, installed::CURSOR)
                    .unwrap();
                StoreReader::new(Storage::new(read.storage).unwrap(), Arc::new(read.history))
            })
            .collect();
        let store = Arc::new(
            Store::new(
                writer,
                history,
                readers,
                2 * 1024 * 1024,
                ReservationBlocks::default(),
                ReadLimits::default(),
            )
            .unwrap(),
        );
        let harness = Harness::new(store.clone(), &fixture.directory, fixture.branch);
        (
            Self {
                fixture,
                store,
                harness,
                writer: Some(handles),
                fresh: Cell::new(128),
            },
            hold,
        )
    }
    /// The Branch's current root, which must be `root`, walked completely
    /// through a fresh bind of a new Workspace that is closed again.
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
    /// The Branch and this Workspace's stage as a separate read-only session
    /// of the Store reads them: no handle of the daemon is consulted.
    fn observed(&self, workspace: WorkspaceId) -> Observed {
        let observer = Handles::open_read_only(
            self.fixture.config.clone(),
            installed::BINDING,
            installed::CURSOR,
        )
        .unwrap();
        let snapshot = observer
            .history
            .branch_snapshot(self.harness.branch)
            .unwrap()
            .expect("the Branch");
        let stage = observer.history.stage(workspace).unwrap();
        Observed {
            head: snapshot.branch.head_commit,
            root: snapshot.effective_root,
            stage,
        }
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
            writer,
            ..
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
            let stopped = owner.stop();
            println!(
                "R5_RIG owner stop with retained custody: {}",
                brief(&stopped)
            );
        }
        println!(
            "R5_RIG store_opened=1 sealed=0 reopened=0 profile=Disposable clean_stop={clean} handles_after_stop={}",
            Arc::strong_count(&store)
        );
        drop((store, writer));
        fixture.cleanup();
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Observed {
    head: Option<CommitId>,
    root: ObjectId,
    stage: Option<StageRecord>,
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
/// Releases the gate when dropped, so a held Commit thread never waits for a
/// test thread that has panicked.
struct Release<'a>(Option<&'a Hold>);
impl Drop for Release<'_> {
    fn drop(&mut self) {
        if let Some(hold) = self.0 {
            hold.release();
        }
    }
}
/// One product control Commit, received and executed once by
/// `Service::serve_one_control` over a real authenticated connection. With a
/// gate, `staged` runs on the test thread while the Commit thread is held at
/// it; what it returns is handed back so the caller ends that effect after
/// the Commit has returned. Returns the original outcome, the reply that was
/// sent, and that value.
fn served<G>(
    fx: &Fx,
    token: WorkspaceToken,
    hold: Option<&Hold>,
    staged: impl FnOnce() -> G,
) -> (Result<Success, Failure>, Reply, G) {
    quiet(&fx.harness, token);
    let service = fx.harness.service.clone();
    let (mut client, worker) =
        support::pair(move |connection| service.serve_one_control(connection));
    // Declared after the worker: dropped, and the gate released, before the
    // worker is joined on any path.
    let release = Release(hold);
    let call = Call {
        id: 1,
        request: Request::Commit(token),
    };
    client.send.send(&call.encode().unwrap()).unwrap();
    if let Some(hold) = hold {
        assert!(hold.entered(WAIT), "the Commit did not reach the gate");
    }
    let effect = staged();
    drop(release);
    // One bounded receive: the connection's own three-second read limit.
    let answer = client.receive.receive().map(Answer::decode);
    let served = worker.join();
    if let Some(hold) = hold {
        assert!(
            !hold.expired(),
            "the Commit left the gate by its limit, not by the release"
        );
    }
    let answer = answer
        .unwrap_or_else(|error| panic!("the reply was not received: {error:?}"))
        .unwrap();
    let served = served.unwrap_or_else(|failure| panic!("served Commit: {failure}"));
    assert_eq!(answer.id, 1);
    assert!(matches!(served.call.request, Request::Commit(served) if served == token));
    (served.outcome, answer.reply, effect)
}
/// The original Commit failure of a served Commit.
fn failed(outcome: Result<Success, Failure>, what: &str) -> Box<CommitFailure> {
    match outcome {
        Err(Failure::Commit(failure)) => failure,
        Ok(done) => panic!("{what}: the Commit succeeded: {:?}", done.reply),
        Err(other) => panic!("{what}: not a Commit failure: {}", brief(&other)),
    }
}
fn refusal(reply: &Reply, what: &str) -> ControlRefusal {
    match reply {
        Reply::Refused(refusal) => refusal.clone(),
        other => panic!("{what}: the reply was {other:?}"),
    }
}
/// A refusal for retained original custody, made before any effect.
fn custody(result: Result<Success, Failure>, what: &str) -> ControlRefusal {
    match result {
        Err(Failure::Custody(refusal)) => *refusal,
        Ok(done) => panic!("{what}: admitted past retained custody: {:?}", done.reply),
        Err(other) => panic!("{what}: {}", brief(&other)),
    }
}
/// The product constructor's request number: the capture's generation.
fn request(failure: &CommitFailure) -> u64 {
    u64::try_from(failure.capture.expect("the capture").generation.number()).unwrap()
}
/// One owner job through the public owner client, attempted once.
fn job(fx: &Fx, route: Route, command: Command) -> Result<Completion, String> {
    fx.harness
        .owner
        .client()
        .try_submit(Some(route), command)
        .map_err(|(cause, _)| format!("unattempted: {cause:?}"))?
        .wait()
        .map_err(|error| format!("{error:?}"))
}
/// What the engine itself holds for one request: the two public `Retained*`
/// owner jobs, each asked once.
fn engine(
    fx: &Fx,
    route: Route,
    request: u64,
) -> Result<(Option<CapturedReader>, Option<OperationOwner>), String> {
    let reader = match job(fx, route, Command::RetainedCapturedReader { request })?.result() {
        Ok(Response::CapturedReader(kept)) => *kept,
        other => return Err(format!("{other:?}")),
    };
    let owner = match job(fx, route, Command::RetainedOperation { request })?.result() {
        Ok(Response::Operation(kept)) => *kept,
        other => return Err(format!("{other:?}")),
    };
    Ok((reader, owner))
}
fn route(fx: &Fx, token: WorkspaceToken) -> Route {
    let operation = fx.harness.service.operation(token).unwrap();
    operation.workspace().route()
}
/// The same mount after the failure: the view is `before`, and one more
/// write by externally launched bash is served and read back.
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
    let made = &after[AFTER.as_bytes()];
    assert_eq!(
        (made.kind, made.mode, made.links, made.payload.as_slice()),
        (
            FILE,
            0o644,
            1,
            b"written after the failed Commit\n".as_slice()
        ),
        "{what}"
    );
    after
}
/// A later Commit and a normal unmount are both refused for the retained
/// custody before any effect: the registry and the mount are as they were.
fn refused_after(fx: &Fx, ready: &ReadyMount, published: &Option<CommitStagedOutcome>, what: &str) {
    let token = ready.token;
    let same = |a: &WorkspaceStatus, b: &WorkspaceStatus| {
        a.epoch == b.epoch
            && a.activity == b.activity
            && a.published == b.published
            && a.binding == b.binding
            && a.local == b.local
    };
    quiet(&fx.harness, token);
    let before = fx.harness.status(token);
    let commit = custody(fx.harness.try_commit(token), "a later Commit");
    let unmount = custody(fx.harness.try_unmount(token), "normal unmount");
    quiet(&fx.harness, token);
    let after = fx.harness.status(token);
    for (name, refusal) in [("later Commit", &commit), ("unmount", &unmount)] {
        assert_eq!(
            (
                refusal.code,
                refusal.phase.as_str(),
                refusal.detail.as_str()
            ),
            (
                ControlCode::Unknown,
                "admission",
                "original Commit custody retained"
            ),
            "{what}: {name}: {refusal:?}"
        );
        assert_eq!(&refusal.published, published, "{what}: {name}");
    }
    assert!(
        same(&before, &after),
        "{what}: a refusal changed the registry"
    );
    assert_eq!(
        after.native.as_ref().map(|native| native.phase),
        Some(NativePhase::Ready),
        "{what}"
    );
    assert!(mount_entry(&ready.directory).is_some(), "{what}");
    println!(
        "{what} REFUSALS later_commit=code:{:?},detail:{:?} unmount=code:{:?} registry_unchanged=true native_phase=Ready mount_table_entry=present",
        commit.code, commit.detail, unmount.code
    );
}
/// Teardown by the test of a mount whose custody the product keeps: one
/// plain umount(8), launched and reaped here, after the connection is quiet.
fn detach(fx: &Fx, ready: &ReadyMount, what: &str) {
    let token = ready.token;
    let detached = Process::new("umount")
        .arg(&ready.directory)
        .output()
        .unwrap();
    assert!(detached.status.success(), "{detached:?}");
    assert!(mount_entry(&ready.directory).is_none());
    let deadline = Instant::now() + Duration::from_secs(3);
    let loops = loop {
        let loops = fx
            .harness
            .status(token)
            .native
            .and_then(|native| native.work)
            .map_or((0, 0), |work| (work.loops_exited, work.loops_configured));
        if (loops.0 == loops.1 && loops.1 != 0) || Instant::now() >= deadline {
            break loops;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    println!(
        "{what} TEARDOWN by_test=plain umount(8) status={:?} mount_table_entry=gone receive_loops_exited={}/{} product_unmount=refused",
        detached.status, loops.0, loops.1
    );
}

/// One external python3 SQLite process, launched, bounded and reaped here.
/// Returns its standard output.
fn external_sqlite(database: &Path, script: &str, arguments: &[String]) -> String {
    struct Reaped(std::process::Child);
    impl Drop for Reaped {
        fn drop(&mut self) {
            if matches!(self.0.try_wait(), Ok(None)) {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }
    let mut child = Reaped(
        Process::new("python3")
            .arg("-c")
            .arg(script)
            .arg(database)
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut status = None;
    until("the external SQLite process exited", || {
        status = child.0.try_wait().unwrap();
        status.is_some()
    });
    let (mut output, mut errors) = (String::new(), String::new());
    child
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    child
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut errors)
        .unwrap();
    assert!(
        status.unwrap().success(),
        "external SQLite process: {status:?}\n{errors}"
    );
    output
}
/// Removes the one location row of an object and prints the row. The Store's
/// own profile setting is used for this connection too; the journal mode is
/// the file's and is not changed.
const REMOVE: &str = r#"
import sqlite3, sys
c = sqlite3.connect(sys.argv[1], timeout=0, isolation_level=None)
c.execute('PRAGMA synchronous=OFF')
key = bytes.fromhex(sys.argv[2])
c.execute('BEGIN IMMEDIATE')
rows = c.execute('SELECT role,canonical_length,pack_id,group_number,record_number FROM object_location WHERE object_id=?', (key,)).fetchall()
assert len(rows) == 1, rows
gone = c.execute('DELETE FROM object_location WHERE object_id=?', (key,)).rowcount
assert gone == 1, gone
c.execute('COMMIT')
left = c.execute('SELECT count(*) FROM object_location WHERE object_id=?', (key,)).fetchone()[0]
assert left == 0, left
c.close()
print(' '.join(str(value) for value in rows[0]))
"#;
/// Puts the row back exactly as it was read.
const RESTORE: &str = r#"
import sqlite3, sys
c = sqlite3.connect(sys.argv[1], timeout=0, isolation_level=None)
c.execute('PRAGMA synchronous=OFF')
key = bytes.fromhex(sys.argv[2])
row = [int(value) for value in sys.argv[3].split()]
assert len(row) == 5, row
c.execute('BEGIN IMMEDIATE')
c.execute('INSERT INTO object_location(object_id,role,canonical_length,pack_id,group_number,record_number) VALUES(?,?,?,?,?,?)', (key, *row))
c.execute('COMMIT')
c.close()
print('restored')
"#;
fn hex(id: ObjectId) -> String {
    use std::fmt::Write;
    id.to_bytes().iter().fold(String::new(), |mut text, byte| {
        write!(text, "{byte:02x}").unwrap();
        text
    })
}
/// A file of the base large enough to be stored as chunks under an extent
/// tree, with no repeated content, so every chunk is its own object.
const NOISE: &str = "big/noise.bin";
const NOISE_BYTES: usize = 1_000_000;
fn noise() -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    (0..NOISE_BYTES)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 32) as u8
        })
        .collect()
}
const PATCH: &[u8] = b"OVERWRITTEN";
/// Which stored object the damage removes, and the change the Commit makes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Victim {
    /// The content root of an untouched file; the Commit changes only
    /// another file's mode. The design of the audit note.
    ContentRoot,
    /// One untouched chunk of a large file; the Commit overwrites a few
    /// bytes of the same file several chunks away, so the rewritten extent
    /// page still names the removed chunk.
    Chunk,
}
/// The untouched file whose content root `Victim::ContentRoot` removes.
const REMOVED: &str = "docs/guide.md";
/// The file whose mode alone that case changes.
const CHANGED: &str = "debug.log";
/// The object to remove and the script of the change, found through the
/// directory, inode and extent pages alone: no file payload is read.
fn plan(fx: &Fx, token: WorkspaceToken, root: ObjectId, victim: Victim) -> (ObjectId, String, u64) {
    let operation = fx.harness.service.operation(token).unwrap();
    let objects = operation.client();
    let mut tree = FilesystemRead::new(objects, FilesystemRootId(root)).unwrap();
    let content = |tree: &mut FilesystemRead<'_>, path: &str| {
        tree.resolve(&LogicalPath::new(path).unwrap())
            .unwrap()
            .value
            .content_root
    };
    let planned = match victim {
        Victim::ContentRoot => (
            content(&mut tree, REMOVED),
            format!("chmod 0640 {CHANGED}"),
            0,
        ),
        Victim::Chunk => {
            let state =
                decode_file_state(&objects.read_canonical(content(&mut tree, NOISE)).unwrap())
                    .unwrap();
            assert_eq!(state.logical_len, NOISE_BYTES as u64);
            // The leftmost extent page: it starts at logical offset 0.
            let mut page = decode_node_with_context(
                &objects.read_canonical(state.mapping_root).unwrap(),
                true,
            )
            .unwrap();
            let extents = loop {
                match page {
                    ExtentNode::Leaf { extents, .. } => break extents,
                    ExtentNode::Branch { children, .. } => {
                        page = decode_node_with_context(
                            &objects.read_canonical(children[0].child_object_id).unwrap(),
                            false,
                        )
                        .unwrap();
                    }
                }
            };
            assert!(extents.len() >= 8, "{} extents on the page", extents.len());
            // The first chunk of the page is removed; the overwrite lands in
            // the middle of the third chunk from the page's end.
            let removed = extents[0].payload_object_id();
            let edited = extents.len() - 3;
            assert!(
                extents[1..]
                    .iter()
                    .all(|extent| extent.payload_object_id() != removed),
                "the removed chunk is used once"
            );
            let offset = extents[..edited]
                .iter()
                .map(|extent| u64::from(extent.logical_length()))
                .sum::<u64>()
                + u64::from(extents[edited].logical_length() / 2);
            assert!(extents[edited].logical_length() as usize > 4 * PATCH.len());
            println!(
                "R5-6 PLAN file={NOISE} bytes={NOISE_BYTES} mapping_level={} extents_on_first_page={} removed_extent=0 (bytes 0..{}) edited_extent={edited} edit_offset={offset}",
                state.tree_level,
                extents.len(),
                extents[0].logical_length()
            );
            (
                removed,
                format!(
                    "printf 'OVERWRITTEN' | dd of={NOISE} bs=1 seek={offset} conv=notrunc status=none"
                ),
                offset,
            )
        }
    };
    assert!(operation.ports().failure().unwrap().is_none());
    planned
}

/// Real Store damage, then one Commit whose new objects still reference the
/// removed object without reading it. The failure must be the Save's
/// dependency check, with no provider read failure; it is definite, settled
/// locally, and both owners are released. With the row put back a later
/// Commit is exact.
fn missing_dependency(name: &str, label: &str, victim: Victim) {
    let fx = Fx::plain(label);
    let ready = fx.harness.mount(1);
    let _mounted = Mounted(ready.directory.clone());
    let mount = root(&ready);
    let token = ready.token;
    let route = route(&fx, token);
    let bound = fx.harness.status(token);
    assert_eq!(bound.activity, Activity::Idle);
    let observed = fx.observed(token.workspace);
    assert_eq!(observed.stage, None);
    let base = native(mount);
    assert_eq!(base[NOISE.as_bytes()].payload, noise());

    // The object the damage removes, found without reading any file.
    let (removed, change, offset) = plan(&fx, token, bound.binding.effective_root, victim);
    // The one change, by externally launched bash, published locally while
    // the Store is whole.
    passed(&bash_in(COMMAND, mount, &change), "the change");
    let mut before = base.clone();
    match victim {
        Victim::ContentRoot => {
            assert_eq!(
                fs::metadata(mount.join(CHANGED)).unwrap().mode() & 0o7777,
                0o640
            );
            before.get_mut(CHANGED.as_bytes()).unwrap().mode = 0o640;
        }
        Victim::Chunk => {
            let changed = fs::metadata(mount.join(NOISE)).unwrap();
            assert_eq!(changed.len(), NOISE_BYTES as u64);
            let seen = before.get_mut(NOISE.as_bytes()).unwrap();
            let at = offset as usize;
            seen.payload[at..at + PATCH.len()].copy_from_slice(PATCH);
            seen.mtime = (changed.mtime(), changed.mtime_nsec() as u32);
        }
    }
    // The damage. Nothing reads the file that names the removed object from
    // here until the row is back.
    let database = fx.fixture.config.path.clone();
    let row = external_sqlite(&database, REMOVE, &[hex(removed)]);
    println!(
        "{name} STAGED how=external python3 SQLite process deleted the object_location row of {victim:?} {} (row: {}) change={change:?} route=serve_one_control(Request::Commit) mount={}",
        hex(removed),
        row.trim(),
        ready.directory
    );

    let (outcome, reply, ()) = served(&fx, token, None, || ());
    let failure = failed(outcome, name);
    let refusal = refusal(&reply, name);
    println!(
        "{name} FAILURE phase={:?} error={} published={:?} locally_settled={} local={} local_error={:?} intent={} namespace={} reply={refusal:?}",
        failure.phase,
        brief(&failure.error),
        failure.published,
        failure.locally_settled,
        failure.local.is_some(),
        failure.local_error,
        failure.intent.is_some(),
        brief(&failure.namespace)
    );
    // The cause is the dependency check: the Save refused an offered object
    // whose direct reference the Store does not locate. It is not a read
    // failure: no provider failure is attached and no reader failed.
    let (object, reference, during_construction) = match &failure.error {
        CommitError::Storage(StorageError::MissingDependency { object, reference }) => {
            (*object, *reference, false)
        }
        CommitError::Construction {
            original,
            storage: StorageError::MissingDependency { object, reference },
        } if matches!(
            **original,
            CommitError::Content {
                error: ContentError::OutputRejected,
                provider: None
            }
        ) =>
        {
            (*object, *reference, true)
        }
        other => panic!(
            "{name}: the cause is not the dependency check: {}",
            brief(other)
        ),
    };
    assert_eq!(reference, removed, "{name}: the missing reference");
    assert_ne!(object, removed);
    assert!(
        fx.store.reader_failures().is_empty(),
        "{name}: a Store reader failed: the cause would be a read"
    );
    assert_eq!(
        failure.phase,
        if during_construction {
            CommitPhase::Construct
        } else {
            CommitPhase::Finish
        }
    );
    // Definite: nothing published, resolved locally, both owners released.
    assert!(failure.published.is_none() && failure.intent.is_none());
    assert!(failure.locally_settled && failure.local.is_none() && failure.local_error.is_none());
    let namespace = failure.namespace.as_ref().expect("the constructor ran");
    assert!(!namespace.retained(), "{namespace:?}");
    assert_eq!(
        namespace.released,
        [ReleasedOwner::Reader, ReleasedOwner::Operation]
    );
    assert!(
        namespace.release_failure.is_none()
            && namespace.release_error.is_none()
            && namespace.custody.is_none()
    );
    assert_eq!(
        (refusal.code, refusal.phase.as_str(), &refusal.published),
        (
            ControlCode::Failed,
            format!("Commit {:?}", failure.phase).as_str(),
            &None
        ),
        "{refusal:?}"
    );
    let request = request(&failure);
    drop(failure);
    assert_eq!(engine(&fx, route, request), Ok((None, None)));
    quiet(&fx.harness, token);
    let status = fx.harness.status(token);
    assert_eq!(
        (status.activity, &status.published, &status.binding),
        (Activity::Idle, &None, &bound.binding)
    );
    assert_eq!(
        fx.observed(token.workspace),
        observed,
        "{name}: no stage and the Branch unchanged"
    );
    // The mount still serves: one more write is published and read back.
    passed(
        &bash_in(
            COMMAND,
            mount,
            &format!("set -euo pipefail; umask 022; printf 'written after the failed Commit\\n' > {AFTER}; cat README.md > /dev/null"),
        ),
        "write after the failed Commit",
    );
    assert_eq!(
        fs::read(mount.join(AFTER)).unwrap(),
        b"written after the failed Commit\n"
    );
    println!(
        "{name} CLASSIFIED cause=MissingDependency reference={} object={} detected={} provider_read_failure=none reader_failures=0 settled=true released=reader,operation activity=Idle stage=None head_unchanged=true mount_serves=true",
        hex(reference),
        hex(object),
        if during_construction {
            "during construction (offer)"
        } else {
            "at Save finish"
        }
    );

    // The row is put back; a later Commit is exact.
    assert_eq!(
        external_sqlite(&database, RESTORE, &[hex(removed), row.trim().to_owned()]),
        "restored\n"
    );
    let expected = native(mount);
    let mut wanted = before.clone();
    wanted.insert(
        AFTER.as_bytes().to_vec(),
        expected[AFTER.as_bytes()].clone(),
    );
    wanted.get_mut(b"".as_slice()).unwrap().mtime = expected[b"".as_slice()].mtime;
    assert_same(
        &wanted,
        &expected,
        "the live view: the one change and one write",
    );
    let done = fx
        .harness
        .try_commit(token)
        .unwrap_or_else(|failure| panic!("{name}: the later Commit: {}", brief(&failure)));
    rig::receipt(&done, name);
    let record = match &done.reply {
        Reply::Committed(CommitStagedOutcome::Committed(record)) => record.clone(),
        other => panic!("{name}: the later Commit replied {other:?}"),
    };
    drop(done);
    assert_eq!(record.parent, bound.binding.branch.head_commit);
    assert_same(&expected, &fx.published(record.root), "the later root");
    assert_same(&expected, &native(mount), "the same mount after install");
    quiet(&fx.harness, token);
    fx.harness.unmount(&ready);
    let fresh = fx.harness.mount(2);
    assert_same(&expected, &native(root(&fresh)), "the fresh mount");
    quiet(&fx.harness, fresh.token);
    fx.harness.unmount(&fresh);
    println!(
        "{name} RESULT row_restored=true later_commit=Committed,exact oracles=published,same_mount,fresh_mount paths={}",
        expected.len()
    );
    fx.finish(true);
}

/// R5-6, missing dependency on the mounted route. Real Store damage: the
/// location row of one untouched chunk of a large file is removed by an
/// external SQLite process. The Commit overwrites a few bytes of the same
/// file several chunks away, so the rewritten extent page still references
/// the removed chunk, and nothing reads it.
#[test]
fn r5_6_a_missing_dependency_from_real_store_damage_is_settled_and_a_later_commit_is_exact() {
    let _serial = serial();
    missing_dependency("R5-6a", "r5-6-missing", Victim::Chunk);
}

/// The design of the audit note, kept as it was first run: the removed object
/// is the content root of an untouched file and the Commit changes another
/// file's mode. It does not reach the dependency check: the Commit succeeds
/// and publishes (receipt `tc-attempt1`). The inode page's constructor
/// attaches only child page identities as references
/// (`layerfs-content/src/filesystem/sorted/page.rs`), so the Save is never
/// offered an untouched inode's content root. Reported to the lead, not
/// judged here; ignored so that the suite states one outcome per run.
#[test]
#[ignore = "reported: the Save's dependency check is not offered an untouched inode's content root"]
fn r5_6_a_reported_an_untouched_inode_content_root_is_not_in_the_dependency_check() {
    let _serial = serial();
    missing_dependency("R5-6a-reported", "r5-6-root", Victim::ContentRoot);
}

/// This process's file-size limit at zero, with `SIGXFSZ` ignored: the kernel
/// refuses every write to a regular file with `EFBIG`. Restored on drop.
struct FileLimit {
    old: libc::rlimit,
    handler: libc::sighandler_t,
}
impl FileLimit {
    fn zero() -> Self {
        let handler = unsafe { libc::signal(libc::SIGXFSZ, libc::SIG_IGN) };
        assert_ne!(handler, libc::SIG_ERR);
        let mut old = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_FSIZE, &mut old) }, 0);
        let zero = libc::rlimit {
            rlim_cur: 0,
            rlim_max: old.rlim_max,
        };
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &zero) }, 0);
        Self { old, handler }
    }
}
impl Drop for FileLimit {
    fn drop(&mut self) {
        let restored = unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &self.old) };
        unsafe { libc::signal(libc::SIGXFSZ, self.handler) };
        if restored != 0 {
            eprintln!("R5_GUARD the file-size limit was not restored");
        }
    }
}
/// The limit is real and is back: a probe file outside the Store.
fn probe(directory: &Path, limited: bool) {
    let path = directory.join("limit-probe");
    let result = fs::write(&path, b"probe");
    match (limited, &result) {
        (true, Err(error)) => assert_eq!(error.raw_os_error(), Some(libc::EFBIG), "{error}"),
        (false, Ok(())) => {}
        other => panic!("file-size limit probe: {other:?}"),
    }
}

/// What a retained failure leaves, common to R5-6b and R5-6c: the failure
/// names the reader and the operation owner and released nothing.
fn kept(failure: &CommitFailure, what: &str) -> (CapturedReader, OperationOwner) {
    assert!(
        !failure.locally_settled && failure.local.is_none() && failure.local_error.is_none(),
        "{what}: no local resolution is attempted: settled={} local={} local_error={:?}",
        failure.locally_settled,
        failure.local.is_some(),
        failure.local_error
    );
    let namespace = failure.namespace.as_ref().expect("the constructor ran");
    assert!(
        namespace.retained()
            && namespace.released.is_empty()
            && namespace.release_failure.is_none()
            && namespace.release_error.is_none()
            && namespace.work.is_some()
            && namespace.counters.is_some(),
        "{what}: {}",
        brief(namespace)
    );
    (
        namespace.reader.expect("the reader is named"),
        namespace.operation.expect("the owner is named"),
    )
}

/// R5-6, an unknown History outcome with nothing published, natively. The
/// Commit thread is held before the real `stage_and_commit`; the file-size
/// limit is set to zero and the gate released. The real provider's own
/// transaction then fails at its write, the provider itself answers
/// `UnknownOutcome`, and nothing was published. The driver attempts no
/// resolution, no install and no resend; the reader and the owner stay.
#[test]
fn r5_6_c_a_native_unknown_history_outcome_with_nothing_published_keeps_custody() {
    let _serial = serial();
    let name = "R5-6c";
    let (fx, hold) = Fx::gated("r5-6-unknown", Point::BeforePublication);
    let ready = fx.harness.mount(1);
    let _mounted = Mounted(ready.directory.clone());
    let mount = root(&ready);
    let token = ready.token;
    let route = route(&fx, token);
    passed(&bash_in(COMMAND, mount, CHANGES), "changes by bash");
    let before = native(mount);
    quiet(&fx.harness, token);
    let bound = fx.harness.status(token);
    assert_eq!(bound.activity, Activity::Idle);
    let observed = fx.observed(token.workspace);
    assert_eq!(
        (observed.head, observed.root, &observed.stage),
        (
            bound.binding.branch.head_commit,
            bound.binding.effective_root,
            &None
        )
    );
    let writer = fx.writer.clone().expect("the writable session");
    let statements = writer.diagnostics().unwrap();
    println!(
        "{name} STAGED how=Commit thread held at support/history_gate.rs Point::BeforePublication; RLIMIT_FSIZE=0 with SIGXFSZ ignored, then released; route=serve_one_control(Request::Commit) mount={} paths={}",
        ready.directory,
        before.len()
    );

    let directory = fx.fixture.directory.clone();
    let (outcome, reply, limit) = served(&fx, token, Some(&hold), || {
        let limit = FileLimit::zero();
        probe(&directory, true);
        limit
    });
    drop(limit);
    probe(&directory, false);
    let failure = failed(outcome, name);
    let refusal = refusal(&reply, name);
    println!(
        "{name} FAILURE phase={:?} error={} published={:?} locally_settled={} local={} local_error={:?} candidate={:?} namespace={} reply={refusal:?}",
        failure.phase,
        brief(&failure.error),
        failure.published,
        failure.locally_settled,
        failure.local.is_some(),
        failure.local_error,
        failure.intent.as_ref().map(|intent| intent.candidate_root),
        brief(&failure.namespace)
    );
    assert!(
        matches!(
            failure.error,
            CommitError::History(HistoryError::UnknownOutcome)
        ),
        "{name}: {}",
        brief(&failure.error)
    );
    assert_eq!(failure.phase, CommitPhase::Publish);
    assert!(failure.published.is_none(), "{name}");
    let candidate = failure.intent.as_ref().expect("the intent").candidate_root;
    assert_ne!(candidate, bound.binding.effective_root);
    let (reader, owner) = kept(&failure, name);
    assert_eq!(
        (refusal.code, refusal.phase.as_str(), &refusal.published),
        (ControlCode::Unknown, "Commit Publish", &None),
        "{refusal:?}"
    );
    // The answer is the real provider's: its own session is quarantined and
    // answers nothing further, and it ran one write transaction that did not
    // commit.
    let after = writer.diagnostics();
    println!(
        "{name} PROVIDER writer_session_before={statements:?} after={}",
        brief(&after)
    );
    let later = writer.history.branch(fx.harness.branch);
    assert!(
        matches!(later, Err(HistoryError::UnknownOutcome)),
        "{name}: the writer session is not quarantined by its own failure: {later:?}"
    );
    // Nothing was published: a separate session reads the Branch and no
    // stage, exactly as before the Commit.
    assert_eq!(
        fx.observed(token.workspace),
        observed,
        "{name}: nothing was published"
    );
    let request = request(&failure);
    assert_eq!(
        engine(&fx, route, request),
        Ok((Some(reader), Some(owner))),
        "{name}: the engine holds the failure's reader and owner"
    );
    quiet(&fx.harness, token);
    let status = fx.harness.status(token);
    assert_eq!(
        (status.activity, &status.published, &status.binding),
        (Activity::Uncertain, &None, &bound.binding)
    );
    assert_eq!(
        status.local.as_ref().and_then(|local| local.captured),
        Some(failure.capture.unwrap().generation.number()),
        "{name}: the capture is retained"
    );
    let live = still_serving(mount, &before, name);
    refused_after(&fx, &ready, &None, name);
    // No refusal and no observation resolved, resent or released anything.
    assert_eq!(engine(&fx, route, request), Ok((Some(reader), Some(owner))));
    assert_eq!(fx.observed(token.workspace), observed);
    println!(
        "{name} CLASSIFIED error=History(UnknownOutcome) native=true published=None nothing_published(independent_session)=true stage=None resolution=none install=none reader=kept owner=kept activity=Uncertain reply_code=Unknown mount_serves=true paths={}",
        live.len()
    );
    quiet(&fx.harness, token);
    detach(&fx, &ready, name);
    drop(failure);
    drop(writer);
    fx.finish(false);
}

/// R5-6, an attempted local install that fails after a known publication,
/// natively. The Commit thread is held after the real `stage_and_commit`
/// returned its known outcome; the file-size limit is set to zero and the
/// gate released. The driver's one `InstallPrepared` is admitted and
/// performed by the engine, and fails at the engine's own write. The
/// publication stays known and published, the reader and the owner stay, and
/// nothing is resolved, resent or released.
#[test]
fn r5_6_b_a_native_attempted_install_failure_after_a_known_publication_keeps_custody() {
    let _serial = serial();
    let name = "R5-6b";
    let (fx, hold) = Fx::gated("r5-6-install", Point::BeforeInstall);
    let ready = fx.harness.mount(1);
    let _mounted = Mounted(ready.directory.clone());
    let mount = root(&ready);
    let token = ready.token;
    let route = route(&fx, token);
    passed(&bash_in(COMMAND, mount, CHANGES), "changes by bash");
    let before = native(mount);
    quiet(&fx.harness, token);
    let bound = fx.harness.status(token);
    assert_eq!(bound.activity, Activity::Idle);
    println!(
        "{name} STAGED how=Commit thread held at support/history_gate.rs Point::BeforeInstall (the real publication has returned); RLIMIT_FSIZE=0 with SIGXFSZ ignored, then released; route=serve_one_control(Request::Commit) mount={} paths={}",
        ready.directory,
        before.len()
    );

    let directory = fx.fixture.directory.clone();
    let (outcome, reply, limit) = served(&fx, token, Some(&hold), || {
        let limit = FileLimit::zero();
        probe(&directory, true);
        limit
    });
    drop(limit);
    probe(&directory, false);
    let failure = failed(outcome, name);
    let refusal = refusal(&reply, name);
    println!(
        "{name} FAILURE phase={:?} error={} published={} locally_settled={} local={} local_error={:?} namespace={} reply={refusal:?}",
        failure.phase,
        brief(&failure.error),
        brief(&failure.published),
        failure.locally_settled,
        failure.local.is_some(),
        failure.local_error,
        brief(&failure.namespace)
    );
    // The install was admitted and performed, and did not answer Done.
    assert_eq!(failure.phase, CommitPhase::Install);
    let installed = match &failure.error {
        CommitError::Completion(done) => done,
        other => panic!("{name}: the install was not attempted: {}", brief(other)),
    };
    let cause = match installed.result() {
        Err(OwnerError::Install { cause, .. }) => format!("{cause:?}"),
        other => panic!("{name}: the install completion: {}", brief(other)),
    };
    println!("{name} INSTALL attempted=true completion=Install cause={cause}");
    // The engine's own answer to the kernel's refusal of its write: an
    // uncertain transaction, with no completion to act on.
    assert!(
        cause.starts_with("Overlay(Uncertain") && cause.contains("SystemIoFailure"),
        "{name}: the install cause: {cause}"
    );
    assert!(!failure.locally_settled && failure.local.is_none());
    // The publication is known.
    let candidate = failure.intent.as_ref().expect("the intent").candidate_root;
    let record = match &failure.published {
        Some(CommitStagedOutcome::Committed(record)) => record.clone(),
        other => panic!("{name}: published {other:?}"),
    };
    assert_eq!(
        (record.root, record.parent),
        (candidate, bound.binding.branch.head_commit)
    );
    let (reader, owner) = kept(&failure, name);
    assert_eq!(
        (refusal.code, refusal.phase.as_str(), &refusal.published),
        (ControlCode::Unknown, "Commit Install", &failure.published),
        "{refusal:?}"
    );
    // An independent session reads the candidate as the Branch root.
    let observed = fx.observed(token.workspace);
    assert_eq!(
        (observed.head, observed.root),
        (Some(record.id), candidate),
        "{name}: the publication is in the Store"
    );
    let status = fx
        .harness
        .service
        .execute_control(&Request::Status(token))
        .map(|done| done.reply);
    let status = match status {
        Ok(Reply::Status(status)) => *status,
        other => panic!("{name}: Status: {}", brief(&other)),
    };
    assert_eq!(
        (status.activity, &status.published, &status.binding),
        (Activity::LocalFailure, &failure.published, &bound.binding),
        "{name}: registry"
    );
    let request = request(&failure);
    let held = engine(&fx, route, request);
    println!(
        "{name} ENGINE after_install_failure retained_owners={} status_local={} status_local_failure={} reader={reader:?} owner={owner:?}",
        brief(&held),
        brief(&status.local),
        brief(&status.local_failure)
    );
    // The engine's own failed transaction quarantined it (first seen in
    // receipt `tc-attempt4`): it answers no job, so the retained owners are
    // named by the failure alone and cannot be read back, and Status carries
    // the engine's refusal in place of a local observation.
    assert!(
        matches!(&held, Err(text) if text.contains("Quarantined")),
        "{name}: the engine after its failed install: {held:?}"
    );
    assert!(status.local.is_none(), "{:?}", status.local);
    let local = status
        .local_failure
        .as_ref()
        .expect("Status names the engine's refusal");
    assert_eq!(
        (local.code, local.phase.as_str(), &local.published),
        (ControlCode::Unknown, "owner completion", &None),
        "{local:?}"
    );
    assert!(local.detail.contains("Quarantined"), "{local:?}");
    // The mount no longer serves: a new write is refused, not lost silently.
    let written = bash_in(
        COMMAND,
        mount,
        &format!("printf 'written after the failed install\\n' > {AFTER}"),
    );
    let stderr = String::from_utf8_lossy(&written.stderr).into_owned();
    println!(
        "{name} MOUNT write_after_failure={:?} stderr={}",
        written.status,
        stderr.trim()
    );
    assert!(
        !written.status.success()
            && (stderr.contains("Transport endpoint is not connected")
                || stderr.contains("Input/output error")),
        "{name}: a write after the failed install: {:?} {stderr}",
        written.status
    );
    let commit = custody(fx.harness.try_commit(token), "a later Commit");
    let unmount = custody(fx.harness.try_unmount(token), "normal unmount");
    for refusal in [&commit, &unmount] {
        assert_eq!(
            (refusal.code, refusal.detail.as_str(), &refusal.published),
            (
                ControlCode::Unknown,
                "original Commit custody retained",
                &failure.published
            )
        );
    }
    assert!(mount_entry(&ready.directory).is_some());
    assert_eq!(
        fx.observed(token.workspace),
        observed,
        "{name}: nothing was resent or discarded"
    );
    println!(
        "{name} CLASSIFIED error=Completion(Install) native=true published=Committed candidate_is_branch_root(independent_session)=true reader=kept owner=kept engine=quarantined mount_serves=false activity=LocalFailure reply_code=Unknown later_commit=refused unmount=refused"
    );
    detach(&fx, &ready, name);
    drop(failure);
    fx.finish(false);
}

/// R5-7, a release attempted and refused after a settled failure. External
/// owner-client scope: while the Commit thread is held before its
/// publication, the test releases the Commit's captured reader through the
/// public owner client. A real external process then holds the Store writer,
/// so the real catalog answers Busy: definite, and the driver's resolution is
/// done. The product's own release of the reader is then performed and
/// refused by the engine. The sequence ends there: the operation owner is not
/// released, both stay named, the registry records the Workspace uncertain,
/// the reply is Unknown, and nothing is retried.
#[test]
fn r5_7_a_release_refused_after_a_settled_failure_keeps_the_owner_and_answers_unknown() {
    let _serial = serial();
    let name = "R5-7";
    let (fx, hold) = Fx::gated("r5-7-release", Point::BeforePublication);
    let ready = fx.harness.mount(1);
    let _mounted = Mounted(ready.directory.clone());
    let mount = root(&ready);
    let token = ready.token;
    let route = route(&fx, token);
    passed(&bash_in(COMMAND, mount, CHANGES), "changes by bash");
    let before = native(mount);
    quiet(&fx.harness, token);
    let bound = fx.harness.status(token);
    assert_eq!(bound.activity, Activity::Idle);
    let observed = fx.observed(token.workspace);
    println!(
        "{name} STAGED how=Commit thread held at support/history_gate.rs Point::BeforePublication; the test releases the Commit's captured reader through the public OwnerClient (a job no product path issues), then an external python3 process holds the Store SQLite writer and the gate is released; route=serve_one_control(Request::Commit) mount={}",
        ready.directory
    );

    let database = fx.fixture.config.path.clone();
    let (outcome, reply, (held, foreign)) = served(&fx, token, Some(&hold), || {
        // Construction has ended; the Commit thread is inside the catalog.
        let committing = fx.harness.status(token);
        assert_eq!(committing.activity, Activity::Committing);
        let generation = committing
            .local
            .as_ref()
            .and_then(|local| local.captured)
            .expect("the capture of the held Commit");
        let request = u64::try_from(generation).unwrap();
        let (reader, owner) = match engine(&fx, route, request) {
            Ok((Some(reader), Some(owner))) => (reader, owner),
            other => panic!("the held Commit's owners: {other:?}"),
        };
        let released = job(&fx, route, Command::ReleaseCapturedReader(reader)).unwrap();
        assert!(
            matches!(released.result(), Ok(Response::Done)),
            "{released:?}"
        );
        drop(released);
        assert_eq!(engine(&fx, route, request), Ok((None, Some(owner))));
        (
            held_writer::HeldWriter::acquire(&database),
            (request, reader, owner),
        )
    });
    held.release();
    let (foreign_request, reader, owner) = foreign;
    let failure = failed(outcome, name);
    let refusal = refusal(&reply, name);
    println!(
        "{name} FAILURE phase={:?} error={} published={:?} locally_settled={} local={} local_error={:?} namespace={} reply={refusal:?}",
        failure.phase,
        brief(&failure.error),
        failure.published,
        failure.locally_settled,
        failure.local.is_some(),
        failure.local_error,
        brief(&failure.namespace)
    );
    // The failure itself is definite and settled.
    assert!(
        matches!(failure.error, CommitError::History(HistoryError::Busy)),
        "{name}: {}",
        brief(&failure.error)
    );
    assert_eq!(failure.phase, CommitPhase::Publish);
    assert!(failure.published.is_none());
    assert!(
        failure.locally_settled && failure.local.is_none() && failure.local_error.is_none(),
        "{name}: the definite failure is resolved locally"
    );
    assert_eq!(request(&failure), foreign_request);
    // The release of the reader was performed and refused; the sequence
    // ended; nothing is released and both owners stay named.
    let namespace = failure.namespace.as_ref().expect("the constructor ran");
    assert!(namespace.retained());
    assert_eq!(
        (namespace.reader, namespace.operation),
        (Some(reader), Some(owner))
    );
    assert!(namespace.released.is_empty(), "{:?}", namespace.released);
    assert!(namespace.release_error.is_none() && namespace.custody.is_none());
    let refused = namespace
        .release_failure
        .as_ref()
        .expect("the refused release's original completion");
    assert!(
        matches!(
            refused.result(),
            Err(OwnerError::Overlay(OverlayError::Stale))
        ),
        "{name}: the release completion: {refused:?}"
    );
    assert!(
        CapturedConstruction::retained(namespace),
        "{name}: custody stays with the failure"
    );
    // The reply must not read as settled.
    assert_eq!(
        (refusal.code, refusal.phase.as_str(), &refusal.published),
        (ControlCode::Unknown, "Commit Publish", &None),
        "{refusal:?}"
    );
    assert!(
        refusal
            .detail
            .starts_with("captured namespace custody retained after: "),
        "{refusal:?}"
    );
    // The engine: the reader is gone (the foreign release), the operation
    // owner is still held and is the one the failure names.
    assert_eq!(
        engine(&fx, route, foreign_request),
        Ok((None, Some(owner))),
        "{name}: the operation owner was not released"
    );
    quiet(&fx.harness, token);
    let status = fx.harness.status(token);
    assert_eq!(
        (status.activity, &status.published, &status.binding),
        (Activity::Uncertain, &None, &bound.binding),
        "{name}: a settled failure with an unreleased owner is recorded uncertain"
    );
    assert_eq!(
        fx.observed(token.workspace),
        observed,
        "{name}: no stage and the Branch unchanged"
    );
    let live = still_serving(mount, &before, name);
    refused_after(&fx, &ready, &None, name);
    // Nothing was retried: the owner is still held, the Branch as it was.
    assert_eq!(engine(&fx, route, foreign_request), Ok((None, Some(owner))));
    assert_eq!(fx.observed(token.workspace), observed);
    println!(
        "{name} CLASSIFIED error=History(Busy) settled=true release(reader)=performed,refused:Stale release(operation)=not_attempted released=[] reader=named owner=named,held_by_engine activity=Uncertain reply_code=Unknown reply_detail_prefix='captured namespace custody retained after: ' later_commit=refused unmount=refused mount_serves=true paths={}",
        live.len()
    );
    quiet(&fx.harness, token);
    detach(&fx, &ready, name);
    drop(failure);
    fx.finish(false);
}
