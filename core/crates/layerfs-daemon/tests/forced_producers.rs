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
//!   Commit thread on a chosen side of the real `stage_and_commit`.
//! - Unknown History custody: `support/history_boundary.rs`
//!   `LostAcknowledgement`, as case F3 of `mounted_commit_failures.rs`.
//!   External catalog boundary scope; not a native SQLite I/O failure.
//! - A known publication with an unattempted install: after the real
//!   publication a catalog wrapper reserves every free admission registration
//!   of the local owner and submits nothing, as case F4.
//! - Attempted work past the loops' exit: both Lifecycle credits of the lane
//!   are held by the test through the public owner client
//!   (`support/holds.rs`), so the reply-ticket release of a published write
//!   cannot be admitted.
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
    ForcedFacts, ForcedOutcome, NativePhase, NativeWork, Reply, Request, TeardownCustody,
    TeardownStage, WorkspaceToken,
};
use layerfs_content::{filesystem::FilesystemRootId, ObjectId};
use layerfs_daemon::{
    bootstrap::open_store,
    control::{Failure, Service, Success},
    store::{CommitError, CommitPhase, ReadLimits, ReleasedOwner, Store, StoreReader},
    Command, Completion, NativeJob, NativeReply, OwnerClient, OwnerError, Response,
};
use layerfs_history::{CommitStagedOutcome, HistoryCatalog, HistoryError, WorkspaceId};
use layerfs_overlay::{CleanupState, NativeMountState, Route};
use layerfs_persistence::Handles;
use layerfs_storage::{port::PackPersistence, ReservationBlocks, Storage};
use model::{assert_same, Flat, Model};
use mounted::{mount_entry, Harness, COMMAND};
use rig::{base_tree, bash_in, native, passed, root};
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

/// External catalog boundary scope: test code at the public History port and
/// the owner's public admission, as `mounted_commit_failures.rs` case F4.
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
                // The loop bound is above the owner's registration capacity.
                let mut held = Saturated {
                    registrations: Vec::new(),
                    refusal: None,
                };
                while held.registrations.len() < 4096 && held.refusal.is_none() {
                    let command = Command::Resources { global: false };
                    match client.submit_when_available(Some(route), command) {
                        Ok(waiting) => held.registrations.push(waiting),
                        Err((cause, _)) => held.refusal = Some(format!("{cause:?}")),
                    }
                }
                *self.held.lock().unwrap() = Some(held);
            }
            Ok(outcome)
        }
    }
}
use catalog_boundary::PublishedThenSaturated;

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
/// One plain umount(8) of an aborted mount, launched and reaped by the test.
fn plain_umount(directory: &str) -> ExitStatus {
    let mut child = Process::new("umount")
        .arg(directory)
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    match exited(&mut child, WAIT) {
        Some(status) => status,
        None => {
            reap(&mut child, "umount");
            panic!("umount {directory} did not return within {WAIT:?}")
        }
    }
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
/// native serving assembly over it: `Harness::new`, as `rig::Rig` uses it.
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
            Model::native(source).1
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
        Self::over(fixture, store)
    }
    fn status_work(&self, token: WorkspaceToken) -> NativeWork {
        self.harness.status(token).native.unwrap().work.unwrap()
    }
    /// Two consecutive observations with nothing received or admitted and no
    /// request completed in between. A readiness wait before one attempt.
    fn quiet(&self, token: WorkspaceToken) {
        let mut last = None;
        within("connection quiescent", || {
            let now = self.status_work(token);
            let settled = now.received == 0 && now.admitted == 0 && last == Some(now.completed);
            last = Some(now.completed);
            thread::sleep(Duration::from_millis(10));
            settled
        });
    }
    /// The owner's count of caller-held results once it is zero, or as it is
    /// after a second. A readiness wait before one attempt; it never fails.
    fn outstanding(&self) -> usize {
        let client = self.harness.owner.client();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let now = client.diagnostics().unwrap().outstanding;
            if now == 0 || Instant::now() >= deadline {
                return now;
            }
            thread::sleep(Duration::from_millis(2));
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
/// One read-only owner job of the test's own, bounded: a readiness wait for
/// a free slot, then the job's completion.
fn job(client: &OwnerClient, route: Route, mut command: Command) -> Completion {
    let deadline = Instant::now() + WAIT;
    let pending = loop {
        match client.try_submit(Some(route), command) {
            Ok(pending) => break pending,
            Err((OwnerError::AdmissionFull, unattempted)) if Instant::now() < deadline => {
                command = unattempted;
                thread::sleep(Duration::from_millis(1));
            }
            Err((error, command)) => panic!("{command:?}: {error:?}"),
        }
    };
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < deadline, "the job did not complete");
        thread::sleep(Duration::from_millis(1));
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
            println!(
                "{name} receipt={} close={} cleanup_observation_failure={:?}",
                brief(&done.native),
                done.completion.is_some(),
                done.observation_failure
            );
            (*outcome, done)
        }
        Ended::Retained(custody) => panic!("{name}: the forced unmount stopped: {custody:?}"),
    }
}
/// A refusal of forced admission: its code and phase, with no completion and
/// no connection evidence, so no engine job and no connection step was made.
fn refused(result: Result<Success, Failure>, what: &str) -> (ControlCode, &'static str, bool) {
    match result {
        Err(Failure::Native(refusal)) => (
            refusal.code,
            refusal.phase,
            refusal.completions.is_empty() && refusal.evidence.is_none(),
        ),
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
        && done.completion.is_some()
        && (work.received, work.admitted, work.retained) == (0, 0, 0)
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
        Request::Unmount(token),
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
    outcome: Result<CommitStagedOutcome, String>,
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
        let request = Request::Commit(token);
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
                            && namespace.release_failure.is_none()
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
        native(&mount)
    });
    let readme = fs::read(mount.join("README.md")).unwrap();
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
    let native_block = after.native.as_ref().unwrap();
    let after_row = row(&ready.directory);
    checks.that(
        during.activity == Activity::Committing
            && after.activity == Activity::Committing
            && after.epoch == during.epoch
            && native_block.phase == NativePhase::Ready
            && !native_block.detached
            && native_block.work.is_some_and(|work| work.terminal == 0)
            && after_row.as_deref() == Some(before_row.as_str())
            && connection.exists(),
        || {
            format!(
                "{name}: the refused Force had an effect: activity {:?} -> {:?}, epoch {} -> {}, phase {:?}, row {after_row:?}, own connection {}",
                during.activity,
                after.activity,
                during.epoch,
                after.epoch,
                native_block.phase,
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
        "{name} held: activity={:?} force_refusals={refusals:?} epoch_unchanged={} native_phase={:?} mount_row=unchanged:{} own_connection={} minor={} mount_serves={served:?} commit_still_held={held}",
        after.activity,
        after.epoch == during.epoch,
        native_block.phase,
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
        (None, Ok(CommitStagedOutcome::UpToDate { head, root })) => checks.that(
            *head == bound.binding.branch.head_commit && *root == bound.binding.effective_root,
            || {
                format!(
                    "{name}: UpToDate names another head or root: {:?}",
                    ended.outcome
                )
            },
        ),
        // The product Commit published exactly the tree captured before it.
        (Some(tree), Ok(CommitStagedOutcome::Committed(record))) => {
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
    let outstanding = fx.outstanding();
    let (outcome, done) = forced(name, &fx.harness, token, false);
    checks.that(
        completed(&outcome, &done) && outcome.facts.commit == CommitKnowledge::Absent,
        || format!("{name}: Force after the Commit ended: {outcome:?}"),
    );
    departed(&fx.harness, &ready.directory, &connection, token);
    println!(
        "{name} after: activity_before_force={:?} owner_outstanding_before_force={outstanding} force=ForceUnmounted abort={:?} detach={:?} commit={:?} cleanup={:?} mount_row=None token=Missing deviations={}",
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
    route: Route,
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
        let route = holds::route(&fx.harness.service, ready.token);
        (
            Self {
                fx,
                token: ready.token,
                directory: ready.directory,
                connection,
                route,
                readme,
            },
            guard,
        )
    }
    /// The closed namespace's cleanup state, asked of the engine by the test.
    fn cleanup(&self) -> String {
        let client = self.fx.harness.owner.client();
        match job(&client, self.route, Command::CleanupState).result() {
            Ok(Response::CleanupState(state)) => format!("{state:?}"),
            other => brief(&other),
        }
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
    let outstanding = fx.outstanding();
    fx.quiet(token);
    let kept = fx.harness.status(token);
    assert_eq!(kept.activity, Activity::Uncertain);
    assert!(kept.published.is_none());
    let kept_row = row(&case.directory).expect("mount row");

    // Without the flag: refused before any effect.
    let refusal = refused(fx.harness.try_force(token, false), name);
    let after = fx.harness.status(token);
    let native_block = after.native.as_ref().unwrap();
    let after_row = row(&case.directory);
    checks.that(
        refusal == (ControlCode::Unknown, "force:custody", true),
        || format!("{name}: Force without the flag answered {refusal:?}"),
    );
    checks.that(
        after.activity == Activity::Uncertain
            && after.epoch == kept.epoch
            && after.published.is_none()
            && native_block.phase == NativePhase::Ready
            && !native_block.detached
            && after_row.as_deref() == Some(kept_row.as_str())
            && case.connection.exists(),
        || {
            format!(
                "{name}: the refused Force had an effect: activity {:?}, epoch {} -> {}, phase {:?}, row {after_row:?}",
                after.activity, kept.epoch, after.epoch, native_block.phase
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
        "{name} without_flag: refusal={refusal:?} activity={:?} epoch_unchanged={} native_phase={:?} mount_row=unchanged:{} own_connection={} mount_serves={served:?} owner_outstanding={outstanding}",
        after.activity,
        after.epoch == kept.epoch,
        native_block.phase,
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
    let engine = case.cleanup();
    checks.that(engine == format!("{:?}", CleanupState::Held), || {
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

/// FP-32, known publication with an unattempted install (as F4): Force needs
/// no flag and its receipt carries the registry's original publication.
#[test]
fn fp32_a_known_publication_is_forced_with_its_original_outcome() {
    let name = "FP-32 known publication";
    fusectl::mount();
    let mut refusing = None;
    let (case, _guard) = Custody::start(Fx::wrapped("fp32-published", |handles, _| {
        let catalog = Arc::new(PublishedThenSaturated {
            handles,
            armed: Mutex::new(None),
            held: Mutex::new(None),
        });
        refusing = Some(catalog.clone());
        catalog
    }));
    let refusing = refusing.expect("catalog boundary");
    let (fx, token) = (&case.fx, case.token);
    let mut checks = Checks::default();
    let bound = fx.harness.status(token);
    *refusing.armed.lock().unwrap() = Some((fx.harness.owner.client(), case.route));
    let result = fx.harness.try_commit(token);
    // The external effect ends here: every registration is dropped unpolled.
    let held = refusing
        .held
        .lock()
        .unwrap()
        .take()
        .expect("the boundary ran");
    let (count, refusal) = (held.registrations.len(), held.refusal.clone());
    drop(held);
    assert!(
        refusal.as_deref() == Some("AdmissionFull") && count > 0,
        "{name}: staging: {count} reserved, refusal {refusal:?}"
    );
    let failure = match result {
        Err(Failure::Commit(failure)) => failure,
        other => panic!("{name}: staging: {}", brief(&other)),
    };
    println!(
        "{name} staged: reserved_registrations={count} phase={:?} error={} published={} locally_settled={}",
        failure.phase,
        brief(&failure.error),
        brief(&failure.published),
        failure.locally_settled
    );
    assert!(
        matches!(
            &failure.error,
            CommitError::Owner(OwnerError::Unattempted { cause, command })
                if matches!(**cause, OwnerError::AdmissionFull)
                    && matches!(**command, Command::InstallPrepared { .. })
        ) && failure.phase == CommitPhase::Install,
        "{name}: staging"
    );
    let original = failure.published.clone();
    match &original {
        Some(CommitStagedOutcome::Committed(record)) => {
            assert_eq!(record.parent, bound.binding.branch.head_commit);
        }
        other => panic!("{name}: staging: published {other:?}"),
    }
    // Dropped before the Force: a kept failure can hold Lifecycle credits.
    drop(failure);
    let outstanding = fx.outstanding();
    fx.quiet(token);
    let kept = fx.harness.status(token);
    assert_eq!(kept.activity, Activity::LocalFailure);
    assert_eq!(kept.published, original, "the registry's publication");

    let (outcome, done) = forced(name, &fx.harness, token, false);
    let published = kept.published.clone().map(CommitKnowledge::Published);
    checks.that(
        completed(&outcome, &done) && Some(&outcome.facts.commit) == published.as_ref(),
        || {
            format!(
                "{name}: Force without the flag: {outcome:?}; the registry's publication {:?}",
                kept.published
            )
        },
    );
    departed(&fx.harness, &case.directory, &case.connection, token);
    let engine = case.cleanup();
    println!(
        "{name} force: relinquish_unknown=false activity_before={:?} owner_outstanding_before={outstanding} force=ForceUnmounted abort={:?} detach={:?} commit_equals_registry_publication={} cleanup={:?} engine_cleanup_state={engine} mount_row=None token=Missing deviations={}",
        kept.activity,
        outcome.facts.abort,
        outcome.facts.detach,
        Some(&outcome.facts.commit) == published.as_ref(),
        outcome.cleanup,
        checks.0.len()
    );
    drop(done);
    drop(refusing);
    case.fx.finish(true);
    checks.done(name);
}

/// The fields of a stored custody that never change; `detached` and `work`
/// are read live from the connection's maintained counters.
fn stable(
    custody: &TeardownCustody,
) -> (WorkspaceToken, TeardownStage, &str, Option<&ForcedFacts>) {
    (
        custody.token,
        custody.stage,
        custody.detail.as_str(),
        custody.forced.as_ref(),
    )
}
/// Every later terminal or attach call on a retained entry, each once: true
/// when each returned the stored custody with no detach known.
fn later(harness: &Harness, token: WorkspaceToken, stored: &TeardownCustody) -> bool {
    [
        Request::ForceUnmount {
            token,
            relinquish_unknown: false,
        },
        Request::ForceUnmount {
            token,
            relinquish_unknown: true,
        },
        Request::Unmount(token),
        Request::Attach(token),
    ]
    .into_iter()
    .all(|request| match harness.service.execute_control(&request) {
        Err(Failure::Retained(custody)) => {
            let equal = stable(&custody) == stable(stored) && !custody.detached;
            if !equal {
                println!("FORCED_DEVIATION {request:?} returned {custody:?}");
            }
            equal
        }
        other => panic!("{request:?} on a retained entry: {}", brief(&other)),
    })
}

/// FP-32, attempted work past the loops' exit. Both Lifecycle credits of the
/// lane are held by the test; an external append is published and answered,
/// and its reply-ticket release cannot be admitted. Force aborts, the loops
/// exit, and the drain stops at the requests that are still owned: no
/// detach, no revocation, no Close, and the write's ticket is kept.
#[test]
fn fp32_held_lifecycle_credits_keep_an_attempted_write_and_the_teardown_retained() {
    let name = "FP-32 held credits";
    fusectl::mount();
    let fx = Fx::plain("fp32-credits");
    let ready = fx.harness.mount(1);
    let _guard = Mounted(ready.directory.clone());
    let token = ready.token;
    let mut checks = Checks::default();
    assert!(ready.receipt.abort_bound, "{:?}", ready.receipt);
    let connection = fusectl::own(&ready);
    let mount = root(&ready).to_path_buf();
    passed(
        &bash_in(
            COMMAND,
            &mount,
            "set -eu; umask 022; mkdir local; printf 'written through the mount\\n' > local/file.txt",
        ),
        "before the hold",
    );
    fx.quiet(token);
    let idle = fx.outstanding();
    let client = fx.harness.owner.client();
    let route = holds::route(&fx.harness.service, token);
    let engine = match job(&client, route, Command::Native(NativeJob::RetainedMount)).result() {
        Ok(Response::Native(NativeReply::RetainedMount(Some(mount)))) => *mount,
        other => panic!("{other:?}"),
    };
    // Publications whose reply attempt is not yet recorded: a Read-class
    // observation, so it is admitted while the Lifecycle credits are held.
    let tickets = || -> Vec<i64> {
        match job(&client, route, Command::PendingPublications { after: 0 }).result() {
            Ok(Response::Publications(pending)) => {
                pending.iter().map(|ticket| ticket.revision()).collect()
            }
            other => panic!("{other:?}"),
        }
    };
    assert!(
        tickets().is_empty(),
        "no publication ticket before the hold"
    );
    let before = fx.harness.status(token);
    let before_row = row(&ready.directory).expect("mount row");

    let credits = holds::Credits::lifecycle(&client, route, 2, WAIT);
    // The attempted work: an ordinary process appends and reads back. Its
    // write is published and answered; its release cannot be admitted.
    let wrote = bash(
        &mount,
        "set -eu; printf 'appended\\n' >> local/file.txt; cat local/file.txt",
    );
    assert!(
        wrote
            .as_ref()
            .is_some_and(|(status, output)| status.success()
                && output == b"written through the mount\nappended\n"),
        "{name}: staging: the append under held credits: {wrote:?}"
    );
    let mut last = None;
    within(
        "the append's requests parked behind the held credits",
        || {
            let now = fx.status_work(token);
            let parked = now.admitted != 0
                && now.admitted == now.parked
                && last == Some((now.admitted, now.completed));
            last = Some((now.admitted, now.completed));
            thread::sleep(Duration::from_millis(10));
            parked
        },
    );
    let parked = fx.status_work(token);
    let held = tickets();
    assert!(
        !held.is_empty(),
        "{name}: staging: the write's publication ticket is still held"
    );
    println!(
        "{name} before: lifecycle_credits_held={} owner_outstanding_idle={idle} append={:?} admitted={} parked={} completed={} pending_publication_revisions={held:?} engine_revision_before_append={:?}",
        credits.count(),
        wrote.as_ref().map(|(status, _)| status.code()),
        parked.admitted,
        parked.parked,
        parked.completed,
        before.local.as_ref().map(|local| local.revision)
    );

    let custody = match force(&fx.harness, token, false) {
        Ended::Retained(custody) => *custody,
        Ended::Unmounted(outcome, _) => {
            panic!("{name}: forced past requests that are still owned: {outcome:?}")
        }
    };
    println!("{name} custody={custody:?}");
    println!(
        "{name} retained_owner={}",
        brief(&fx.harness.service.retained_native(token))
    );
    let facts = custody.forced.clone();
    checks.that(
        custody.token == token
            && custody.stage == TeardownStage::Requests
            && !custody.detached
            && facts.as_ref().is_some_and(|facts| {
                facts.abort == AbortDisposition::Written
                    && facts.detach == DetachDisposition::NotAttempted
                    && facts.commit == CommitKnowledge::Absent
            }),
        || {
            format!(
                "{name}: not Retained at Requests with abort Written and no detach: {custody:?}"
            )
        },
    );
    // The loops exited and were joined; every request that was owned before
    // the abort is still owned: the fence ended none of them.
    checks.that(
        custody.work.is_some_and(|work| {
            work.loops_joined == work.loops_configured
                && work.loops_configured != 0
                && work.admitted >= parked.admitted
                && work.received == 0
        }),
        || format!("{name}: the connection at the stop: {:?}", custody.work),
    );
    let stopped = fx.harness.status(token);
    let stopped_row = row(&ready.directory);
    let kept = tickets();
    checks.that(
        stopped_row.as_deref() == Some(before_row.as_str())
            && connection.exists()
            && stopped.activity == Activity::Closing
            && stopped
                .native
                .as_ref()
                .is_some_and(|native| native.phase == NativePhase::Retained && !native.detached),
        || {
            format!(
                "{name}: after the stop: row {stopped_row:?}, activity {:?}, native {:?}",
                stopped.activity,
                stopped.native.as_ref().map(|native| native.phase)
            )
        },
    );
    // The attempted write keeps its result and its guard.
    checks.that(kept == held, || {
        format!("{name}: the publication tickets changed: {held:?} -> {kept:?}")
    });
    // A later call returns the stored custody: no second abort, no detach.
    let equal_held = later(&fx.harness, token, &custody);
    checks.that(
        equal_held && row(&ready.directory).as_deref() == Some(before_row.as_str()),
        || format!("{name}: a later call, credits still held, had an effect"),
    );
    println!(
        "{name} after_force: stage={:?} abort={:?} detach={:?} detached={} fenced={:?} mount_row=unchanged:{} own_connection={} phase={:?} activity={:?} pending_publication_revisions={kept:?} later_calls_equal_custody={equal_held}",
        custody.stage,
        facts.as_ref().map(|facts| facts.abort),
        facts.as_ref().map(|facts| facts.detach),
        custody.detached,
        facts.as_ref().map(|facts| facts.fenced),
        stopped_row.as_deref() == Some(before_row.as_str()),
        connection.exists(),
        stopped.native.as_ref().map(|native| native.phase),
        stopped.activity
    );

    // The hold ends. The owned requests dispose themselves; the engine mount
    // is still live, the namespace open, and nothing is detached afterwards.
    credits.release();
    within(
        "the owned requests completed once the credits returned",
        || {
            let now = fx.status_work(token);
            now.admitted == 0 && now.received == 0
        },
    );
    let mount_state = match job(&client, route, Command::Native(NativeJob::State(engine))).result()
    {
        Ok(Response::Native(NativeReply::State(state))) => Some(*state),
        other => {
            println!("{name} engine mount state: {}", brief(&other));
            None
        }
    };
    let namespace = match job(&client, route, Command::CleanupState).result() {
        Ok(Response::CleanupState(state)) => Some(*state),
        other => {
            println!("{name} cleanup state: {}", brief(&other));
            None
        }
    };
    let released = tickets();
    let last_status = fx.harness.status(token);
    let equal_released = later(&fx.harness, token, &custody);
    let last_row = row(&ready.directory);
    checks.that(
        mount_state == Some(NativeMountState::Live)
            && namespace == Some(CleanupState::Live)
            && last_status.local.as_ref().is_some_and(|local| !local.closed),
        || {
            format!(
                "{name}: the engine after the stop: mount {mount_state:?}, namespace {namespace:?}, local {:?}",
                last_status.local
            )
        },
    );
    checks.that(
        equal_released && last_row.as_deref() == Some(before_row.as_str()),
        || format!("{name}: a later call, credits returned, had an effect: row {last_row:?}"),
    );
    println!(
        "{name} after_release: engine_mount={mount_state:?} namespace={namespace:?} closed={:?} engine_revision={:?} pending_publication_revisions={released:?} admitted={:?} later_calls_equal_custody={equal_released} mount_row=unchanged:{} registry_phase={:?}",
        last_status.local.as_ref().map(|local| local.closed),
        last_status.local.as_ref().map(|local| local.revision),
        last_status
            .native
            .as_ref()
            .and_then(|native| native.work)
            .map(|work| work.admitted),
        last_row.as_deref() == Some(before_row.as_str()),
        last_status.native.as_ref().map(|native| native.phase)
    );

    // Teardown by the test: the product never detaches this mount.
    let detached = plain_umount(&ready.directory);
    assert!(detached.success(), "{name}: umount: {detached:?}");
    assert!(mount_entry(&ready.directory).is_none());
    println!(
        "{name} teardown: by_test=plain umount(8) status={:?} mount_row=None deviations={}",
        detached.code(),
        checks.0.len()
    );
    drop(client);
    fx.finish(false);
    checks.done(name);
}
