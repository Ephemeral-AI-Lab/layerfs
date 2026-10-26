//! Real kernel mounts: forced teardown against control producers and the
//! custody they leave (FP-32).
//!
//! Staging, hook-free, at the same public boundaries the R5 failure matrix
//! uses:
//!
//! - A Commit held before Save finish: `Service::execute` with a constructor
//!   closure that blocks on a bounded gate and then returns the bound root.
//! - A Commit held before publication and before its local install: the
//!   product control Commit over `support/history_gate.rs`, which holds the
//!   Commit thread on a chosen side of the real `commit_branch`.
//! - Unknown History custody: `support/history_boundary.rs`
//!   `LostAcknowledgement`, as case F3 of `mounted_commit_failures.rs`.
//!   External catalog boundary scope; not a kernel SQLite I/O failure.
//! - A known publication with an unattempted install: after the real
//!   publication a catalog wrapper reserves every free admission registration
//!   of the local owner and submits nothing, as case F4.
//! - Attempted work past the loops' exit: both Lifecycle credits of the lane
//!   are held by the test through the public owner client
//!   (`support/holds.rs`), so the releases that follow an answered append
//!   and read-back (the descriptor's close, the read sources) cannot be
//!   admitted. The write's reply ticket needs no owner job and is returned.
//!
//! A failed Commit's `Failure` is dropped before any Force: it can hold
//! Lifecycle completions, and the forced Revoke and Close do not wait for a
//! credit. The held-credit case is the one that keeps such a credit on
//! purpose.
//!
//! Every wait is bounded. A held Commit thread proceeds by itself when its
//! limit expires, and its hold is released by a guard on every path. Nothing
//! is timed. Not staged: a cold-read consumer held inside a provider read
//! (no hook-free hold exists), and the held-credit variant in which the
//! credits are returned inside the drain window.
//!
//! The safety rule of `support/fusectl.rs` binds this file: nothing here
//! writes under `/sys/fs/fuse/connections` or reads another connection.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/fusectl.rs"]
mod fusectl;
#[allow(dead_code)]
#[path = "support/held_writer.rs"]
mod held_writer;
#[allow(dead_code)]
#[path = "support/history_boundary.rs"]
mod history_boundary;
#[allow(dead_code)]
#[path = "support/history_gate.rs"]
mod history_gate;
#[allow(dead_code)]
#[path = "support/holds.rs"]
mod holds;
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
use history_gate::Point;
use layerfs_bridge::control::{
    AbortDisposition, Activity, CommitKnowledge, ControlCode, DetachDisposition, ForcedCleanup,
    ForcedOutcome, MountPhase, MountWork, Reply, Request, TeardownCustody, WorkspaceToken,
};
use layerfs_content::{filesystem::FilesystemRootId, ObjectId};
use layerfs_daemon::{
    bootstrap::open_store,
    control::{Failure, Service, Success},
    store::{CommitError, CommitPhase, ReleasedOwner, Store, StoreReader},
};
use layerfs_history::{CommitOutcome, HistoryCatalog, HistoryError, WorkspaceId};
use layerfs_overlay::CleanupState;
use layerfs_persistence::Handles;
use layerfs_storage::{port::PackPersistence, ReservationBlocks, Storage};
use model::{assert_same, Flat, Model};
use mounted::{mount_entry, Harness, COMMAND};
use rig::{base_tree, bash_in, host, passed, root};
use std::{
    cell::Cell,
    fmt::Debug,
    fs,
    io::Read,
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command as Process, ExitStatus, Stdio},
    sync::{atomic::AtomicBool, Arc, Condvar, Mutex, MutexGuard},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Every bounded readiness or completion observation of the test.
const WAIT: Duration = Duration::from_secs(5);
/// How long a held Commit thread stays held if nothing releases it.
const LIMIT: Duration = Duration::from_secs(20);
/// Ordinary changes of several kinds, run with the mount as the working
/// directory by a process nothing registered.
const CHANGES: &str = r#"
set -euo pipefail
umask 022
mkdir -p made/sub
printf 'created before the Commit\n' > made/new.txt
printf 'appended line\n' >> README.md
mv -T src/main.rs made/sub/main-moved.rs
ln -s ../new.txt made/sub/rel-link
rm -f spare/gone.txt
"#;
/// The file an external process writes while the mount must still serve.
const SERVED: &str = "served-meanwhile.txt";

fn within(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "bounded observation expired: {what}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
/// A bounded description of an original value for an evidence line.
fn brief(value: &impl Debug) -> String {
    let mut text = format!("{value:?}");
    let mut end = text.len().min(700);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}
/// Product expectations that did not hold. Reported after every hold is
/// released and every mount is gone, so a deviation leaves its whole evidence.
#[derive(Default)]
struct Checks(Vec<String>);
impl Checks {
    fn that(&mut self, holds: bool, what: impl FnOnce() -> String) {
        if !holds {
            let what = what();
            println!("FORCED_DEVIATION {what}");
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
/// Never leaves a kernel mount behind: if the test unwinds, one lazy
/// umount(8) launched and reaped by the test detaches what is still there.
struct Mounted(String);
impl Drop for Mounted {
    fn drop(&mut self) {
        if mount_entry(&self.0).is_some() {
            let done = Process::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "FORCED_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}
/// The child's exit status, or `None` if it has not exited within `wait`.
fn exited(child: &mut Child, wait: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + wait;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
            _ => return None,
        }
    }
}
/// A process that did not end within its bound: kill, then a bounded reap.
fn reap(child: &mut Child, what: &str) {
    let _ = child.kill();
    if exited(child, Duration::from_secs(3)).is_none() {
        println!(
            "FORCED_GUARD {what} (pid {}) was killed and is not reaped",
            child.id()
        );
    }
}
/// Real `bash -c` of the command identity with the mount as its working
/// directory, launched by plain exec and registered with nothing. Its exit
/// status and standard output, or `None` if it did not end within `WAIT`; it
/// is reaped either way.
fn bash(directory: &Path, script: &str) -> Option<(ExitStatus, Vec<u8>)> {
    let mut child = Process::new("bash")
        .args(["-c", script])
        .uid(COMMAND)
        .gid(COMMAND)
        .current_dir(directory)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let Some(status) = exited(&mut child, WAIT) else {
        reap(&mut child, "bash");
        return None;
    };
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    Some((status, output))
}
/// This mount namespace's whole mount-table row of `directory`.
fn row(directory: &str) -> Option<String> {
    let table = fs::read_to_string("/proc/self/mountinfo").unwrap();
    table
        .lines()
        .rev()
        .find(|line| {
            line.split_once(" - ")
                .is_some_and(|(before, _)| before.split(' ').nth(4) == Some(directory))
        })
        .map(str::to_owned)
}

/// One installed Disposable Store, opened once and kept open, and one real
/// kernel serving assembly over it: `Harness::new`, as `rig::Rig` uses it.
struct Fx {
    fixture: installed::Fixture,
    store: Arc<Store>,
    harness: Harness,
    fresh: Cell<u8>,
}
impl Fx {
    fn fixture(label: &str) -> installed::Fixture {
        installed::Fixture::built(label, |source| {
            base_tree(source);
            Model::host(source).1
        })
    }
    fn over(fixture: installed::Fixture, store: Arc<Store>) -> Self {
        let harness = Harness::new(store.clone(), &fixture.directory, fixture.branch);
        Self {
            fixture,
            store,
            harness,
            fresh: Cell::new(128),
        }
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
        Self::over(fixture, store)
    }
    /// The same public pieces `open_store` composes, with the History port
    /// chosen by the caller around the real catalog: one writable session,
    /// two read-only sessions, one Store.
    fn wrapped(
        label: &str,
        history: impl FnOnce(Arc<Handles>, &Path) -> Arc<dyn HistoryCatalog>,
    ) -> Self {
        let fixture = Self::fixture(label);
        let config = || fixture.config.clone();
        let handles = Arc::new(
            Handles::open_writable(config(), installed::BINDING, installed::CURSOR).unwrap(),
        );
        let writer: Arc<dyn PackPersistence> = handles.storage.clone();
        let history = history(handles, &fixture.config.path);
        let reader_config = fixture.config.clone();
        let factory: layerfs_daemon::store::ReaderFactory = Arc::new(move |_| {
            let read = Handles::open_read_only(
                reader_config.clone(),
                installed::BINDING,
                installed::CURSOR,
            )
            .unwrap();
            Ok(StoreReader::new(
                Storage::new(read.storage).unwrap(),
                Arc::new(read.history),
            ))
        });
        let control = factory(0).unwrap();
        let store = Arc::new(
            Store::new(
                writer,
                history,
                control,
                factory,
                2 * 1024 * 1024,
                ReservationBlocks::default(),
                16,
            )
            .unwrap(),
        );
        Self::over(fixture, store)
    }
    fn status_work(&self, token: WorkspaceToken) -> MountWork {
        self.harness.status(token).kernel.unwrap().work.unwrap()
    }
    /// Two consecutive observations with nothing received or admitted and no
    /// request completed in between. A readiness wait before one attempt.
    fn quiet(&self, token: WorkspaceToken) {
        let mut last = None;
        within("connection quiescent", || {
            let now = self.status_work(token);
            let settled = now.active == 0 && last == Some(now.completed);
            last = Some(now.completed);
            thread::sleep(Duration::from_millis(10));
            settled
        });
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
    /// `clean`: no mount and no retained session remains, so the serving
    /// assembly is stopped with its checks. Otherwise the registry still owns
    /// a retained session that nothing can settle; the kernel mount was
    /// detached by the test and the assembly is dropped with that custody.
    fn finish(self, clean: bool) {
        let Self {
            fixture,
            store,
            harness,
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
            owner.stop().unwrap();
        }
        println!(
            "FORCED_RIG store_opened=1 sealed=0 reopened=0 profile=Disposable clean_stop={clean} handles_after_stop={}",
            Arc::strong_count(&store)
        );
        drop(store);
        fixture.cleanup();
    }
}
/// One read and one write by an ordinary external process: the base file's
/// bytes come back and the new file is served to this process too.
fn serves(mount: &Path, readme: &[u8], what: &str) -> Result<(), String> {
    let script = format!("set -eu; cat README.md; printf '{what}\\n' > {SERVED}");
    match bash(mount, &script) {
        Some((status, output)) if status.success() && output == readme => {}
        other => return Err(format!("read and write: {other:?}")),
    }
    match fs::read(mount.join(SERVED)) {
        Ok(bytes) if bytes == format!("{what}\n").as_bytes() => Ok(()),
        other => Err(format!("{SERVED}: {other:?}")),
    }
}
/// What one forced unmount ended as.
enum Ended {
    Unmounted(Box<ForcedOutcome>, Box<Success>),
    /// Stopped after an effect; the registry keeps the exact owner.
    Retained(Box<TeardownCustody>),
}
/// One forced terminal unmount, attempted once.
fn force(harness: &Harness, token: WorkspaceToken, relinquish_unknown: bool) -> Ended {
    match harness.try_force(token, relinquish_unknown) {
        Ok(done) => match &done.reply {
            Reply::ForceUnmounted(closed) => {
                assert_eq!(closed.token, token);
                Ended::Unmounted(Box::new(closed.outcome.clone()), Box::new(done))
            }
            other => panic!("forced unmount replied {other:?}"),
        },
        Err(Failure::Retained(custody)) => Ended::Retained(custody),
        Err(other) => panic!("forced unmount: {}", brief(&other)),
    }
}
/// A forced unmount that must complete; its receipt is printed.
fn forced(
    name: &str,
    harness: &Harness,
    token: WorkspaceToken,
    relinquish_unknown: bool,
) -> (ForcedOutcome, Box<Success>) {
    match force(harness, token, relinquish_unknown) {
        Ended::Unmounted(outcome, done) => {
            println!("{name} outcome={outcome:?}");

            (*outcome, done)
        }
        Ended::Retained(custody) => panic!("{name}: the forced unmount stopped: {custody:?}"),
    }
}
/// A refusal of forced admission: its code and phase, with no completion and
/// no connection evidence, so no engine job and no connection step was made.
fn refused(result: Result<Success, Failure>, what: &str) -> (ControlCode, &'static str, bool) {
    match result {
        Err(Failure::Kernel(refusal)) => (refusal.code, refusal.phase, refusal.evidence.is_none()),
        other => panic!("{what}: {}", brief(&other)),
    }
}
/// The abort was written, the one detach succeeded and the drain is whole.
fn completed(outcome: &ForcedOutcome, done: &Success) -> bool {
    let work = outcome.work;
    outcome.facts.abort == AbortDisposition::Written
        && outcome.facts.detach == DetachDisposition::Detached
        && outcome.cleanup != ForcedCleanup::Unobserved
        && done.observation_failure.is_none()
        && (work.active, work.retained) == (0, 0)
        && work.loops_joined == work.loops_configured
}
/// Independent kernel observations of a completed teardown, and routing.
fn departed(
    harness: &Harness,
    directory: &str,
    connection: &fusectl::Connection,
    token: WorkspaceToken,
) {
    assert!(mount_entry(directory).is_none());
    assert!(!Path::new(directory).exists());
    within("own connection directory removed", || !connection.exists());
    for request in [
        Request::ForceUnmount {
            token,
            relinquish_unknown: true,
        },
        Request::Unmount(layerfs_bridge::control::UnmountControl {
            token,
            discard: true,
        }),
        Request::Status(token),
    ] {
        assert!(
            matches!(
                harness.service.execute_control(&request),
                Err(Failure::Rejected(ControlCode::Missing, _))
            ),
            "{request:?}"
        );
    }
}

#[derive(Default)]
struct GateState {
    entered: bool,
    released: bool,
    expired: bool,
}
/// One bounded hold of a constructor closure.
#[derive(Default)]
struct Gate {
    state: Mutex<GateState>,
    changed: Condvar,
}
impl Gate {
    fn lock(&self) -> MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
    /// Blocks the calling Commit thread until release or its own limit.
    fn hold(&self) {
        let mut state = self.lock();
        state.entered = true;
        self.changed.notify_all();
        let deadline = Instant::now() + LIMIT;
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
    fn entered(&self, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        let mut state = self.lock();
        while !state.entered {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            state = self
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
        true
    }
    fn release(&self) {
        self.lock().released = true;
        self.changed.notify_all();
    }
}
/// The test's side of a held Commit. Dropping it releases the Commit thread.
enum Hold {
    Constructor(Arc<Gate>),
    History(history_gate::Hold),
}
impl Hold {
    fn entered(&self) -> bool {
        match self {
            Self::Constructor(gate) => gate.entered(WAIT),
            Self::History(hold) => hold.entered(WAIT),
        }
    }
    fn release(&self) {
        match self {
            Self::Constructor(gate) => gate.release(),
            Self::History(hold) => hold.release(),
        }
    }
    /// True if the Commit thread left by its own limit, not by a release.
    fn expired(&self) -> bool {
        match self {
            Self::Constructor(gate) => gate.lock().expired,
            Self::History(hold) => hold.expired(),
        }
    }
}
impl Drop for Hold {
    fn drop(&mut self) {
        self.release();
    }
}
/// What a Commit thread reports: plain values, the original receipts dropped
/// on that thread.
struct Finished {
    /// The Commit's own reply, or its failure.
    outcome: Result<CommitOutcome, String>,
    /// The product constructor's receipt: reader, then owner, both released
    /// and nothing retained. Absent when a caller's constructor ran.
    released: Option<bool>,
}
/// One Commit on its own thread, attempted once. With a gate it runs a
/// caller constructor that is held before the Save finishes and then returns
/// the bound root; otherwise it is the product control Commit.
fn commit(
    service: Arc<Service>,
    token: WorkspaceToken,
    gate: Option<Arc<Gate>>,
) -> JoinHandle<Finished> {
    thread::spawn(move || {
        let request = Request::Commit(layerfs_bridge::control::CommitControl {
            token,
            force: false,
            message: None,
        });
        let result = match gate {
            Some(gate) => service.execute(&request, move |_, _, snapshot| {
                gate.hold();
                Ok(FilesystemRootId(snapshot.effective_root))
            }),
            None => service.execute_control(&request),
        };
        match result {
            Ok(done) => Finished {
                outcome: match &done.reply {
                    Reply::Committed(outcome) => Ok(outcome.clone()),
                    other => Err(format!("{other:?}")),
                },
                released: done
                    .commit
                    .as_ref()
                    .and_then(|commit| commit.namespace.as_ref())
                    .map(|namespace| {
                        !namespace.retained()
                            && namespace.released
                                == [ReleasedOwner::Reader, ReleasedOwner::Operation]
                            && namespace.release_error.is_none()
                            && namespace.release_error.is_none()
                            && namespace.custody.is_none()
                    }),
            },
            Err(failure) => Finished {
                outcome: Err(brief(&failure)),
                released: None,
            },
        }
    })
}

/// FP-32, active producer: while a Commit is held at `point`, Force is
/// refused Busy before any effect, the mount keeps serving, and the Commit
/// then ends with its own outcome. `point` absent holds a caller constructor
/// before the Save finishes.
fn busy_while_a_commit_is_held(name: &'static str, label: &str, point: Option<Point>) {
    fusectl::mount();
    let (fx, hold, gate) = match point {
        None => {
            let gate = Arc::new(Gate::default());
            (
                Fx::plain(label),
                Hold::Constructor(gate.clone()),
                Some(gate),
            )
        }
        Some(point) => {
            let mut side = None;
            let fx = Fx::wrapped(label, |handles, _| {
                let (catalog, hold) = history_gate::gated(handles, point, LIMIT);
                side = Some(hold);
                Arc::new(catalog)
            });
            (fx, Hold::History(side.expect("gate")), None)
        }
    };
    let ready = fx.harness.mount(1);
    let _guard = Mounted(ready.directory.clone());
    let token = ready.token;
    let mut checks = Checks::default();
    assert!(ready.receipt.abort_bound, "{:?}", ready.receipt);
    let connection = fusectl::own(&ready);
    let mount = root(&ready).to_path_buf();
    // The product constructor commits real changes; the caller constructor
    // returns the bound root, so its mount is left unchanged before it.
    let tree = point.map(|_| {
        passed(
            &bash_in(COMMAND, &mount, CHANGES),
            "changes by externally launched bash",
        );
        host(&mount)
    });
    let readme = fs::read(mount.join("README.md")).unwrap();
    // An unchanged mount is answered before capture (#352 c04). Rewriting a
    // file with its own bytes gives the caller constructor a capture to hold.
    if point.is_none() {
        fs::write(mount.join("README.md"), &readme).unwrap();
    }
    fx.quiet(token);
    let bound = fx.harness.status(token);
    assert_eq!(bound.activity, Activity::Idle);
    let before_row = row(&ready.directory).expect("mount row");

    let running = commit(fx.harness.service.clone(), token, gate);
    assert!(hold.entered(), "{name}: the Commit did not reach its hold");
    let during = fx.harness.status(token);
    let mut refusals = Vec::new();
    for relinquish_unknown in [false, true] {
        let refusal = refused(fx.harness.try_force(token, relinquish_unknown), name);
        checks.that(
            refusal == (ControlCode::Busy, "force:admission", true),
            || {
                format!(
                    "{name}: Force (relinquish_unknown={relinquish_unknown}) answered {refusal:?}"
                )
            },
        );
        refusals.push(refusal);
    }
    // No effect: the same serving connection, activity and epoch, the same
    // mount row and the test's own connection entry.
    let after = fx.harness.status(token);
    let kernel_block = after.kernel.as_ref().unwrap();
    let after_row = row(&ready.directory);
    checks.that(
        during.activity == Activity::Committing
            && after.activity == Activity::Committing
            && after.epoch == during.epoch
            && kernel_block.phase == MountPhase::Ready
            && !kernel_block.detached
            && kernel_block.work.is_some_and(|work| work.terminal == 0)
            && after_row.as_deref() == Some(before_row.as_str())
            && connection.exists(),
        || {
            format!(
                "{name}: the refused Force had an effect: activity {:?} -> {:?}, epoch {} -> {}, phase {:?}, row {after_row:?}, own connection {}",
                during.activity,
                after.activity,
                during.epoch,
                after.epoch,
                kernel_block.phase,
                connection.exists()
            )
        },
    );
    let served = serves(&mount, &readme, "during the hold");
    checks.that(served.is_ok(), || {
        format!("{name}: the mount did not serve during the hold: {served:?}")
    });
    let held = !hold.expired() && !running.is_finished();
    checks.that(held, || {
        format!("{name}: the Commit was not held throughout")
    });
    println!(
        "{name} held: activity={:?} force_refusals={refusals:?} epoch_unchanged={} kernel_phase={:?} mount_row=unchanged:{} own_connection={} minor={} mount_serves={served:?} commit_still_held={held}",
        after.activity,
        after.epoch == during.epoch,
        kernel_block.phase,
        after_row.as_deref() == Some(before_row.as_str()),
        connection.exists(),
        connection.minor
    );

    hold.release();
    within("the Commit ended after its release", || {
        running.is_finished()
    });
    let ended = running.join().expect("the Commit thread");
    println!(
        "{name} commit_after_release: outcome={} constructor_owners_released={:?}",
        brief(&ended.outcome),
        ended.released
    );
    fx.quiet(token);
    let idle = fx.harness.status(token);
    match (&tree, &ended.outcome) {
        // The caller constructor returned the bound root.
        (None, Ok(CommitOutcome::UpToDate { head, root })) => checks.that(
            *head == bound.binding.branch.head_commit && *root == bound.binding.effective_root,
            || {
                format!(
                    "{name}: UpToDate names another head or root: {:?}",
                    ended.outcome
                )
            },
        ),
        // The product Commit published exactly the tree captured before it.
        (Some(tree), Ok(CommitOutcome::Committed { record, .. })) => {
            checks.that(
                record.parent == bound.binding.branch.head_commit
                    && record.root != bound.binding.effective_root
                    && ended.released == Some(true)
                    && idle.binding.effective_root == record.root
                    && idle.binding.branch.head_commit == Some(record.id),
                || {
                    format!(
                        "{name}: the Commit's own outcome: {record:?}, owners released {:?}, bound root {:?}",
                        ended.released, idle.binding.effective_root
                    )
                },
            );
            let published = fx.published(record.root);
            assert_same(
                tree,
                &published,
                &format!("{name}: the Commit's root, fresh bind"),
            );
            println!(
                "{name} published: parent={:?} paths={} oracle=fresh_bind held_write_absent={}",
                record.parent,
                published.len(),
                !published.contains_key(SERVED.as_bytes())
            );
        }
        (_, other) => checks.that(false, || {
            format!("{name}: the Commit did not end with its own outcome: {other:?}")
        }),
    }
    // The write made during the hold is a later mutation: still served.
    let kept = fs::read(mount.join(SERVED));
    checks.that(
        idle.activity == Activity::Idle
            && idle.published.is_none()
            && kept
                .as_ref()
                .is_ok_and(|bytes| bytes == b"during the hold\n"),
        || {
            format!(
                "{name}: after the Commit: activity {:?}, published {:?}, the held write {kept:?}",
                idle.activity, idle.published
            )
        },
    );

    // The same Workspace is forced once its Commit has ended.
    let (outcome, done) = forced(name, &fx.harness, token, false);
    checks.that(
        completed(&outcome, &done) && outcome.facts.commit == CommitKnowledge::Absent,
        || format!("{name}: Force after the Commit ended: {outcome:?}"),
    );
    departed(&fx.harness, &ready.directory, &connection, token);
    println!(
        "{name} after: activity_before_force={:?} force=ForceUnmounted abort={:?} detach={:?} commit={:?} cleanup={:?} mount_row=None token=Missing deviations={}",
        idle.activity,
        outcome.facts.abort,
        outcome.facts.detach,
        outcome.facts.commit,
        outcome.cleanup,
        checks.0.len()
    );
    drop(done);
    drop(hold);
    fx.finish(true);
    checks.done(name);
}

#[test]
fn fp32_force_is_busy_while_a_commit_is_held_before_save_finish() {
    busy_while_a_commit_is_held("FP-32 before Save finish", "fp32-construct", None);
}

#[test]
fn fp32_force_is_busy_while_a_commit_is_held_before_publication() {
    busy_while_a_commit_is_held(
        "FP-32 before publication",
        "fp32-publication",
        Some(Point::BeforePublication),
    );
}

#[test]
fn fp32_force_is_busy_while_a_commit_is_held_before_its_local_install() {
    busy_while_a_commit_is_held(
        "FP-32 before install",
        "fp32-install",
        Some(Point::BeforeInstall),
    );
}

/// What a custody case has when its Commit is about to be attempted.
struct Custody {
    fx: Fx,
    token: WorkspaceToken,
    directory: String,
    connection: fusectl::Connection,
    readme: Vec<u8>,
}
impl Custody {
    /// The mount's guard is returned second so that it is dropped first.
    fn start(fx: Fx) -> (Self, Mounted) {
        let ready = fx.harness.mount(1);
        let guard = Mounted(ready.directory.clone());
        assert!(ready.receipt.abort_bound, "{:?}", ready.receipt);
        let connection = fusectl::own(&ready);
        let mount = root(&ready);
        passed(
            &bash_in(COMMAND, mount, CHANGES),
            "changes by externally launched bash",
        );
        let readme = fs::read(mount.join("README.md")).unwrap();
        fx.quiet(ready.token);
        assert_eq!(fx.harness.status(ready.token).activity, Activity::Idle);
        (
            Self {
                fx,
                token: ready.token,
                directory: ready.directory,
                connection,
                readme,
            },
            guard,
        )
    }
    /// The closed namespace's cleanup state, asked of the engine by the test.
    fn cleanup(&self) -> String {
        let client = self.fx.harness.owner.client();
        let state = client
            .observe_cleanup(None, self.token.namespace, self.token.workspace.to_bytes())
            .unwrap();
        format!("{state:?}")
    }
}

/// FP-32, unknown History custody (as F3): without the flag Force is refused
/// Unknown before any effect; with it the Workspace is closed, the receipt
/// still says Unknown and the engine custody stays held.
#[test]
fn fp32_unknown_commit_custody_is_forced_only_when_relinquished_and_stays_unknown() {
    let name = "FP-32 unknown custody";
    fusectl::mount();
    let (case, _guard) = Custody::start(Fx::wrapped("fp32-unknown", |handles, database| {
        Arc::new(ObservedHistory {
            handles,
            database: database.to_path_buf(),
            boundary: Boundary::LostAcknowledgement,
            once: AtomicBool::new(true),
        })
    }));
    let (fx, token) = (&case.fx, case.token);
    let mut checks = Checks::default();
    let bound = fx.harness.status(token);
    let failure = match fx.harness.try_commit(token) {
        Err(Failure::Commit(failure)) => failure,
        other => panic!("{name}: staging: {}", brief(&other)),
    };
    println!(
        "{name} staged: phase={:?} error={} published={:?} locally_settled={}",
        failure.phase,
        brief(&failure.error),
        failure.published,
        failure.locally_settled
    );
    assert!(
        matches!(
            failure.error,
            CommitError::History(HistoryError::UnknownOutcome)
        ) && failure.phase == CommitPhase::Publish
            && failure.published.is_none()
            && !failure.locally_settled,
        "{name}: staging"
    );
    let candidate = failure.intent.as_ref().map(|intent| intent.candidate_root);
    // Dropped before any Force: a kept failure can hold Lifecycle credits.
    drop(failure);
    fx.quiet(token);
    let kept = fx.harness.status(token);
    assert_eq!(kept.activity, Activity::Uncertain);
    assert!(kept.published.is_none());
    let kept_row = row(&case.directory).expect("mount row");

    // Without the flag: refused before any effect.
    let refusal = refused(fx.harness.try_force(token, false), name);
    let after = fx.harness.status(token);
    let kernel_block = after.kernel.as_ref().unwrap();
    let after_row = row(&case.directory);
    checks.that(
        refusal == (ControlCode::Unknown, "force:custody", true),
        || format!("{name}: Force without the flag answered {refusal:?}"),
    );
    checks.that(
        after.activity == Activity::Uncertain
            && after.epoch == kept.epoch
            && after.published.is_none()
            && kernel_block.phase == MountPhase::Ready
            && !kernel_block.detached
            && after_row.as_deref() == Some(kept_row.as_str())
            && case.connection.exists(),
        || {
            format!(
                "{name}: the refused Force had an effect: activity {:?}, epoch {} -> {}, phase {:?}, row {after_row:?}",
                after.activity, kept.epoch, after.epoch, kernel_block.phase
            )
        },
    );
    let served = serves(
        Path::new(&case.directory),
        &case.readme,
        "after the refusal",
    );
    checks.that(served.is_ok(), || {
        format!("{name}: the mount did not serve after the refusal: {served:?}")
    });
    println!(
        "{name} without_flag: refusal={refusal:?} activity={:?} epoch_unchanged={} kernel_phase={:?} mount_row=unchanged:{} own_connection={} mount_serves={served:?}",
        after.activity,
        after.epoch == kept.epoch,
        kernel_block.phase,
        after_row.as_deref() == Some(kept_row.as_str()),
        case.connection.exists()
    );

    // With the flag: closed, still Unknown, the engine custody still held.
    fx.quiet(token);
    let (outcome, done) = forced(name, &fx.harness, token, true);
    checks.that(
        completed(&outcome, &done)
            && outcome.facts.commit == CommitKnowledge::Unknown
            && outcome.cleanup == ForcedCleanup::Held,
        || format!("{name}: Force with relinquish_unknown: {outcome:?}"),
    );
    departed(&fx.harness, &case.directory, &case.connection, token);
    // The Force reply above observed the closed namespace Held while its
    // connection was open. After relinquishment the closed backing is disposed;
    // the original unknown Commit result remains retained independently.
    let engine = case.cleanup();
    checks.that(engine == format!("{:?}", CleanupState::Gone), || {
        format!("{name}: the engine reports the closed namespace {engine}")
    });
    // The receipt is not settled by a read: an independent session of the
    // Store sees the candidate published, and the receipt still says Unknown.
    let observer = Handles::open_read_only(
        fx.fixture.config.clone(),
        installed::BINDING,
        installed::CURSOR,
    )
    .unwrap();
    let observed = observer
        .history
        .branch_snapshot(fx.harness.branch)
        .unwrap()
        .unwrap();
    drop(observer);
    assert!(
        Some(observed.effective_root) == candidate
            && observed.effective_root != bound.binding.effective_root,
        "{name}: staging: the lost acknowledgement's publication happened"
    );
    println!(
        "{name} with_flag: force=ForceUnmounted abort={:?} detach={:?} commit={:?} cleanup={:?} fenced={} engine_cleanup_state={engine} independent_session_root=candidate receipt_settled_by_read=false mount_row=None token=Missing deviations={}",
        outcome.facts.abort,
        outcome.facts.detach,
        outcome.facts.commit,
        outcome.cleanup,
        outcome.facts.fenced,
        checks.0.len()
    );
    drop(done);
    case.fx.finish(true);
    checks.done(name);
}
