//! Real kernel mounts: what a terminal unmount waits for and what it retires
//! afterwards (FP-21), a processing read that spans the release of a sibling
//! handle and the removal of its name (FP-29), and exact FORGET decrements
//! with bounded retirement of lookups still outstanding at detach (FP-31).
//!
//! Staging, hook-free, through `support/holds.rs`:
//!
//! - FP-21 holds both Lifecycle credits of the Workspace's owner lane as kept
//!   completions of the test's own `State` observations. A descriptor closed
//!   afterwards sends RELEASE, whose `close_file` job cannot be admitted, so
//!   the request is parked before its attempt while normal Unmount detaches
//!   and joins the receive loop.
//! - FP-29 leases every Store reader. A READ of a file whose bytes are still
//!   the base's parks on reader admission. The file was moved into a
//!   directory made through the mount and its mode changed before the hold,
//!   so its name, parent and inode rows are local and removing the name asks
//!   the base for nothing.
//! - FP-31 asks the kernel for reclaim of the test container's own memory
//!   cgroup only, which makes it evict unreferenced dentries and inodes and
//!   send FORGET on a connection that stays mounted. No system-wide cache is
//!   dropped. Its partial decrement holds the Lifecycle pair so that a FORGET
//!   unit is parked while a new LOOKUP of the same name raises the count.
//!
//! Observed, by counts: the mount's maintained counters through control
//! Status, the engine's maintained row counts and point states through
//! ordinary owner jobs, the owner's maintenance counters, the drain receipt's
//! opcode counts and the kernel's own answers. Nothing is timed.
//!
//! Not staged here and recorded as such: a Store consumer held by the test
//! is not a namespace-bound consumer, so Close has nothing of it to wait for
//! (FP-21); a FORGET of a foreign incarnation and an underflowing FORGET stay
//! at component scope (FP-31).
#![cfg(target_os = "linux")]
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
use layerfs_bridge::control::{
    Activity, ControlCode, NativePhase, NativeWork, ReadyMount, Reply, Request, TeardownCustody,
    TeardownStage, WorkspaceToken,
};
use layerfs_daemon::{
    control::Failure, Command, Completion, NativeJob, NativeReply, OwnerClient, OwnerError,
    OwnerWork, Response,
};
use layerfs_fuse::request::Opcode;
use layerfs_overlay::{
    CleanupState, NativeMount, NativeMountState, Route, StatementKind, StoredCounts,
};
use mounted::{mount_entry, COMMAND};
use nix::{errno::Errno, libc};
use rig::{bash_in, passed, pattern, root, stamp_tree, Rig};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{FileExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    process::Command as Process,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Every bounded readiness or completion observation of the tests.
const WAIT: Duration = Duration::from_secs(5);
/// Bound of one retire-until-gone observation.
const LONG: Duration = Duration::from_secs(20);
/// Bound of one reclaim-until-returned observation. How many requests the
/// kernel needs before it walks its dentry and inode lists is its own
/// business: in attempt 1 the two thousand inodes were not all returned
/// within 20 s, and attempt 2 needed 162 requests.
const RECLAIM: Duration = Duration::from_secs(40);
/// How long a request is watched before it is recorded as not having
/// completed while a hold was in place.
const HELD: Duration = Duration::from_secs(3);
/// Rows one maintenance turn may retire.
const WINDOW: u64 = 64;

fn before(what: &str, wait: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + wait;
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "bounded observation expired: {what}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn within(what: &str, condition: impl FnMut() -> bool) {
    before(what, WAIT, condition)
}
/// Never leaves a kernel mount behind: if the test unwinds, one lazy
/// umount(8) launched and reaped by the test detaches what is still there.
struct Mounted(String);
impl Drop for Mounted {
    fn drop(&mut self) {
        if mount_entry(&self.0).is_some() {
            let done = Process::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "DRAIN_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}
/// Expectations that did not hold, reported after the hold is returned and
/// the mount is torn down, so a deviation leaves its whole evidence.
#[derive(Default)]
struct Checks(Vec<String>);
impl Checks {
    fn that(&mut self, holds: bool, what: impl FnOnce() -> String) {
        if !holds {
            let what = what();
            println!("DRAIN_DEVIATION {what}");
            self.0.push(what);
        }
    }
    fn done(self, name: &str) {
        assert!(
            self.0.is_empty(),
            "{name}: {} expectations did not hold: {:#?}",
            self.0.len(),
            self.0
        );
    }
}

fn work(rig: &Rig, token: WorkspaceToken) -> NativeWork {
    rig.harness.status(token).native.unwrap().work.unwrap()
}
/// The mount's counters once `reached` holds, or as they are after a second.
/// Never fails; the caller checks what it got.
fn settled(rig: &Rig, token: WorkspaceToken, reached: impl Fn(&NativeWork) -> bool) -> NativeWork {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let now = work(rig, token);
        if reached(&now) || Instant::now() >= deadline {
            return now;
        }
        thread::sleep(Duration::from_millis(2));
    }
}
/// Two consecutive observations with nothing received or admitted and no
/// request completed in between.
fn quiet(rig: &Rig, token: WorkspaceToken) {
    let mut last = None;
    within("connection quiescent", || {
        let now = work(rig, token);
        let settled = now.received == 0 && now.admitted == 0 && last == Some(now.completed);
        last = Some(now.completed);
        thread::sleep(Duration::from_millis(10));
        settled
    });
}
/// One owner job of the test, attempted once after a bounded wait for a free
/// slot of its class, and awaited within the same bound.
fn job(client: &OwnerClient, route: Route, mut command: Command) -> Completion {
    let deadline = Instant::now() + WAIT;
    let pending = loop {
        match client.try_submit(Some(route), command) {
            Ok(pending) => break pending,
            Err((OwnerError::AdmissionFull, back)) => {
                assert!(
                    Instant::now() < deadline,
                    "owner observation {back:?}: no free slot: {:?}",
                    client.diagnostics()
                );
                command = back;
                thread::sleep(Duration::from_millis(1));
            }
            Err((error, back)) => panic!("owner observation {back:?}: {error:?}"),
        }
    };
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(
            Instant::now() < deadline,
            "owner observation did not complete: {:?}",
            client.diagnostics()
        );
        thread::sleep(Duration::from_millis(1));
    }
}
/// The engine's retained native mount of this namespace, if one is attached.
fn engine_mount(client: &OwnerClient, route: Route) -> Option<NativeMount> {
    let done = job(client, route, Command::Native(NativeJob::RetainedMount));
    match done.result() {
        Ok(Response::Native(NativeReply::RetainedMount(mount))) => *mount,
        other => panic!("{other:?}"),
    }
}
fn mount_state(client: &OwnerClient, route: Route, mount: NativeMount) -> NativeMountState {
    let done = job(client, route, Command::Native(NativeJob::State(mount)));
    match done.result() {
        Ok(Response::Native(NativeReply::State(state))) => *state,
        other => panic!("{other:?}"),
    }
}
/// One maintained point observation of the namespace's close and cleanup.
fn cleanup(client: &OwnerClient, route: Route) -> CleanupState {
    let done = job(client, route, Command::CleanupState);
    match done.result() {
        Ok(Response::CleanupState(state)) => *state,
        other => panic!("{other:?}"),
    }
}
/// Per-opcode frame counts out of a Debug-rendered drain receipt.
fn opcodes(receipt: &str) -> Vec<u64> {
    let key = "OpcodeWork { opcodes: [";
    let at = receipt.find(key).unwrap_or_else(|| panic!("{receipt}")) + key.len();
    let end = at + receipt[at..].find(']').unwrap();
    receipt[at..end]
        .split(", ")
        .map(|count| count.parse().unwrap())
        .collect()
}
/// One named counter out of a Debug-rendered receipt.
fn counter(receipt: &str, name: &str) -> u64 {
    let key = format!("{name}: ");
    let at = receipt
        .find(&key)
        .unwrap_or_else(|| panic!("{name}: {receipt}"));
    receipt[at + key.len()..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap()
}
/// Attributes the kernel fetched from the daemon for this call, never the
/// kernel's cached copy: (inode, link count, size).
fn forced(file: &File) -> Result<(u64, u32, u64), Errno> {
    let mut raw = std::mem::MaybeUninit::<libc::statx>::zeroed();
    Errno::result(unsafe {
        libc::statx(
            file.as_raw_fd(),
            c"".as_ptr(),
            libc::AT_EMPTY_PATH | libc::AT_STATX_FORCE_SYNC,
            libc::STATX_BASIC_STATS,
            raw.as_mut_ptr(),
        )
    })?;
    let raw = unsafe { raw.assume_init() };
    Ok((raw.stx_ino, raw.stx_nlink, raw.stx_size))
}

/// What one normal Unmount returned, reduced on its own thread.
enum Left {
    Unmounted {
        /// The reply was exactly `Unmounted` of the token.
        exact: bool,
        earlier: usize,
        closed: bool,
        receipt: String,
    },
    Retained(Box<TeardownCustody>),
    Other(String),
}
/// One normal Unmount, attempted once on its own thread.
fn unmount_apart(rig: &Rig, token: WorkspaceToken) -> (Receiver<Left>, JoinHandle<()>) {
    let service = rig.harness.service.clone();
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        let left = match service.execute_control(&Request::Unmount(token)) {
            Ok(done) => Left::Unmounted {
                exact: done.reply == Reply::Unmounted(token),
                earlier: done.earlier.len(),
                closed: done.completion.is_some(),
                receipt: format!("{:?}", done.native),
            },
            Err(Failure::Retained(custody)) => Left::Retained(custody),
            Err(other) => Left::Other(format!("{other:?}")),
        };
        let _ = sender.send(left);
    });
    (receiver, worker)
}

/// One mounted Workspace with a helper Workspace bound beside it for the
/// engine's daemon-wide counts, the state before it existed, one descriptor
/// closed while both Lifecycle credits of its lane are held, and so its
/// RELEASE parked before the admission of its `close_file` job.
struct Parked {
    credits: holds::Credits,
    /// Dropped after the credits: a lazy unmount of what an unwinding test left.
    _guard: Mounted,
    ready: ReadyMount,
    helper: WorkspaceToken,
    route: Route,
    mount: NativeMount,
    /// Daemon-wide counts with only the helper bound.
    empty: StoredCounts,
    start: OwnerWork,
    parked: NativeWork,
    client: OwnerClient,
    rig: Rig,
}
impl Parked {
    fn new(label: &str) -> Self {
        let rig = Rig::new(label);
        let client = rig.harness.owner.client();
        let helper = rig.harness.bind(1);
        within("the owner is idle after the bind", || {
            client.diagnostics().unwrap().outstanding == 0
        });
        let empty = rig.harness.engine(helper);
        let start = client.diagnostics().unwrap();
        let ready = rig.mount(2);
        let guard = Mounted(ready.directory.clone());
        let token = ready.token;
        let route = holds::route(&rig.harness.service, token);
        let mut file = File::open(root(&ready).join("README.md")).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"# base\n");
        quiet(&rig, token);
        let mount = engine_mount(&client, route).expect("attached engine mount");
        assert_eq!(mount_state(&client, route, mount), NativeMountState::Live);
        assert_eq!(cleanup(&client, route), CleanupState::Live);
        let mounted = rig.harness.engine(helper);
        assert_eq!(mounted.namespaces, empty.namespaces + 1);
        assert!(mounted.owner_rows > empty.owner_rows, "{mounted:?}");
        quiet(&rig, token);
        let before = work(&rig, token);
        let credits = holds::Credits::lifecycle(&client, route, 2, WAIT);
        drop(file);
        within("the RELEASE is parked before admission", || {
            let now = work(&rig, token);
            now.parked == 1 && now.admitted == 1
        });
        let parked = work(&rig, token);
        assert_eq!(
            (parked.handoffs - before.handoffs, parked.received),
            (1, 0),
            "one RELEASE handed off: {before:?} -> {parked:?}"
        );
        assert_eq!(
            (
                parked.loops_configured,
                parked.loops_entered,
                parked.loops_exited
            ),
            (1, 1, 0)
        );
        println!(
            "FP-21 staged: lifecycle_credits_held={} owner_outstanding={} release(handoffs={}->{} admitted={} parked={} completed={}) engine(namespaces={} owner_rows={}) mount_state=Live cleanup=Live",
            credits.count(),
            client.diagnostics().unwrap().outstanding,
            before.handoffs,
            parked.handoffs,
            parked.admitted,
            parked.parked,
            parked.completed,
            mounted.namespaces,
            mounted.owner_rows
        );
        Self {
            credits,
            _guard: guard,
            ready,
            helper,
            route,
            mount,
            empty,
            start,
            parked,
            client,
            rig,
        }
    }
}

/// FP-21, released inside the window. Detach is known and the loop is
/// joined while the parked RELEASE keeps the unmount in its drain and the
/// namespace live; once the test returns the credits inside the bounded drain
/// window the same Unmount ends `Unmounted`. After that complete drain the
/// engine's counts return to what they were before the Workspace existed and
/// its cleanup state reaches `Gone`.
#[test]
fn fp21_a_parked_release_keeps_unmount_draining_until_its_credits_return_inside_the_window() {
    // Bound in this order so that an unwinding test returns the credits
    // first, then detaches what is still mounted, then drops the rig.
    let Parked {
        rig,
        client,
        parked,
        start,
        empty,
        mount,
        route,
        helper,
        ready,
        _guard: guard,
        credits,
    } = Parked::new("fp21-released");
    let token = ready.token;
    let (left, worker) = unmount_apart(&rig, token);
    within("detach known and the loop joined", || {
        let native = rig.harness.status(token).native.unwrap();
        let work = native.work.unwrap();
        native.phase == NativePhase::Draining
            && native.detached
            && work.loops_joined == work.loops_configured
    });
    // Detach and joined loops alone retire nothing: at every one of fifty
    // observations the unmount is still draining, the request is still
    // parked and the namespace still exists.
    let mut steady = 0;
    for round in 0..50 {
        let status = rig.harness.status(token);
        let native = status.native.unwrap();
        let now = native.work.unwrap();
        assert_eq!(status.activity, Activity::Closing);
        assert_eq!(native.phase, NativePhase::Draining, "{now:?}");
        assert_eq!(
            (now.loops_joined, now.admitted, now.parked, now.received),
            (1, 1, 1, 0),
            "{now:?}"
        );
        if round % 10 == 0 {
            let counts = rig.harness.engine(helper);
            assert_eq!(counts.namespaces, empty.namespaces + 1);
            assert!(counts.owner_rows > empty.owner_rows, "{counts:?}");
        }
        steady += 1;
        thread::sleep(Duration::from_millis(2));
    }
    assert!(mount_entry(&ready.directory).is_none(), "detached");
    assert!(
        matches!(left.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "the Unmount has not returned"
    );
    let waiting = work(&rig, token);
    println!(
        "FP-21 released: phase=Draining detached=true loops_joined={} steady_observations={steady} admitted={} parked={} completed={}->{} mount_row=None",
        waiting.loops_joined,
        waiting.admitted,
        waiting.parked,
        parked.completed,
        waiting.completed
    );

    credits.release();
    let done = left
        .recv_timeout(WAIT)
        .expect("the Unmount did not return after the credits were released");
    worker.join().unwrap();
    let receipt = match done {
        Left::Unmounted {
            exact,
            earlier,
            closed,
            receipt,
        } => {
            assert!(exact, "the reply is Unmounted of this token: {receipt}");
            assert_eq!(earlier, 1, "native-owner revocation receipt");
            assert!(closed, "logical Close receipt");
            receipt
        }
        Left::Retained(custody) => panic!("released inside the window, yet {custody:?}"),
        Left::Other(other) => panic!("{other}"),
    };
    assert!(receipt.contains("Drained"), "{receipt}");
    assert!(mount_entry(&ready.directory).is_none());
    assert!(!root(&ready).exists());
    assert!(matches!(
        rig.harness.service.execute_control(&Request::Status(token)),
        Err(Failure::Rejected(ControlCode::Missing, _))
    ));
    let counts = opcodes(&receipt);
    println!(
        "FP-21 released: reply=Unmounted revocation_receipts=1 close_receipt=true receipt_opcodes(open={} read={} release={}) receipt={receipt}",
        counts[Opcode::Open as usize],
        counts[Opcode::Read as usize],
        counts[Opcode::Release as usize]
    );
    assert_eq!(counts[Opcode::Release as usize], 1, "{receipt}");

    // The complete drain: bounded indexed retirement until Gone.
    let mut states = Vec::new();
    before("cleanup state Gone", LONG, || {
        let state = cleanup(&client, route);
        if states.last() != Some(&state) {
            states.push(state);
        }
        state == CleanupState::Gone
    });
    let mut last = rig.harness.engine(helper);
    before("engine counts returned to their baseline", LONG, || {
        last = rig.harness.engine(helper);
        last == empty
    });
    let end = client.diagnostics().unwrap();
    println!(
        "FP-21 released: cleanup_states={states:?} counts_equal_baseline=true baseline={empty:?} closed_namespaces={}->{} maintenance_jobs={}->{} maintenance_rows={}->{} engine_mount_before={mount:?}",
        start.closed_namespaces,
        end.closed_namespaces,
        start.maintenance_jobs,
        end.maintenance_jobs,
        start.maintenance_rows,
        end.maintenance_rows
    );
    assert_eq!(end.closed_namespaces, start.closed_namespaces + 1);
    assert!(client.maintenance_failure().unwrap().is_none());
    drop(guard);
    let closed = rig.harness.try_unmount(helper).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(helper));
    drop(closed);
    rig.finish();
}

/// FP-21, not released. With the credits still held when the bounded drain
/// window ends, the Unmount stops `Retained` at stage `Requests` after its
/// detach and both loop joins: the engine mount is still live and the
/// namespace is not closed. Later calls answer with the same custody, and
/// returning the credits afterwards settles nothing by itself.
#[test]
fn fp21_credits_held_past_the_window_leave_unmount_retained_at_requests_with_a_live_engine_mount() {
    // Bound in this order so that an unwinding test returns the credits
    // first, then detaches what is still mounted, then drops the rig.
    let Parked {
        rig,
        client,
        parked,
        start,
        empty,
        mount,
        route,
        helper,
        ready,
        _guard: guard,
        credits,
    } = Parked::new("fp21-retained");
    let token = ready.token;
    let (left, worker) = unmount_apart(&rig, token);
    // The window is the serving assembly's drain wait of three seconds.
    let done = left
        .recv_timeout(WAIT * 2)
        .expect("the Unmount did not return at the end of its drain window");
    worker.join().unwrap();
    let custody = match done {
        Left::Retained(custody) => *custody,
        Left::Unmounted { receipt, .. } => {
            panic!("unmounted with a request still parked: {receipt}")
        }
        Left::Other(other) => panic!("{other}"),
    };
    println!("FP-21 retained: custody={custody:?}");
    assert_eq!(custody.token, token);
    assert_eq!(custody.stage, TeardownStage::Requests);
    assert!(custody.detached, "the kernel detach is known");
    assert!(custody.forced.is_none());
    assert!(!custody.detail.is_empty());
    let stopped = custody.work.expect("counters at the stopping boundary");
    assert_eq!(
        (stopped.loops_configured, stopped.loops_joined),
        (1, 1),
        "{stopped:?}"
    );
    assert_eq!(
        (
            stopped.admitted,
            stopped.parked,
            stopped.received,
            stopped.retained
        ),
        (1, 1, 0, 0),
        "{stopped:?}"
    );
    assert_eq!(stopped.completed, parked.completed, "nothing completed");
    // Effects that happened stay happened; nothing is reported as unmounted.
    assert!(mount_entry(&ready.directory).is_none());
    let status = rig.harness.status(token);
    assert_eq!(status.activity, Activity::Closing);
    let native = status.native.unwrap();
    assert_eq!(native.phase, NativePhase::Retained);
    assert!(native.detached);
    assert_eq!(native.ready.as_ref(), Some(&ready));
    let kept = rig
        .harness
        .service
        .retained_native(token)
        .unwrap()
        .expect("retained owner");
    assert!(kept.contains("Requests"), "{kept}");
    // Later operations answer with the same custody and attempt nothing.
    // Nothing has released an owner credit since the stop, so the parked
    // request was not polled and the whole record is equal.
    for request in [Request::Unmount(token), Request::Attach(token)] {
        match rig.harness.service.execute_control(&request) {
            Err(Failure::Retained(again)) => assert_eq!(*again, custody, "{request:?}"),
            other => panic!("{request:?}: {other:?}"),
        }
    }
    // The namespace is not closed and nothing of it was retired, observed
    // while the credits are still held.
    let held = rig.harness.engine(helper);
    assert_eq!(held.namespaces, empty.namespaces + 1);
    assert!(held.owner_rows > empty.owner_rows, "{held:?}");
    // Observation: the custody's counters are read from the retained
    // connection at each reply, not stored at the stop. The test's own owner
    // job above released a credit, which wakes the parked request for one
    // more refused admission, and a reply made then can show it mid-step
    // (attempt 1). What a parked request cannot change is compared.
    match rig.harness.try_unmount(token) {
        Err(Failure::Retained(again)) => {
            assert_eq!(
                (
                    again.token,
                    again.stage,
                    again.detached,
                    &again.detail,
                    &again.forced
                ),
                (
                    custody.token,
                    custody.stage,
                    custody.detached,
                    &custody.detail,
                    &custody.forced
                )
            );
            let now = again.work.expect("counters of the retained connection");
            assert_eq!(
                (
                    now.loops_joined,
                    now.admitted,
                    now.received,
                    now.retained,
                    now.completed,
                    now.handoffs
                ),
                (1, 1, 0, 0, stopped.completed, stopped.handoffs),
                "{now:?}"
            );
            println!(
                "FP-21 retained: later_replies_equal=2 reply_after_an_owner_job(identical={} queued={} running={} parked={})",
                *again == custody,
                now.queued,
                now.running,
                now.parked
            );
        }
        other => panic!("{other:?}"),
    }
    let serving = rig.harness.serving.work().unwrap();
    assert_eq!(serving.mounts, 1, "the undrained lane is kept: {serving:?}");

    // Returning the credits lets the parked RELEASE finish. No hidden retry
    // of the teardown runs: the entry stays Retained at the same stage.
    credits.release();
    within("the parked RELEASE completed", || {
        let now = work(&rig, token);
        now.admitted == 0 && now.received == 0
    });
    let after = work(&rig, token);
    assert_eq!(after.completed, parked.completed + 1);
    assert_eq!((after.retained, after.terminal), (0, 0));
    assert_eq!(
        engine_mount(&client, route),
        Some(mount),
        "the engine mount is neither revoked nor replaced"
    );
    assert_eq!(mount_state(&client, route, mount), NativeMountState::Live);
    assert_eq!(cleanup(&client, route), CleanupState::Live);
    assert_eq!(rig.harness.phase(token), NativePhase::Retained);
    match rig.harness.try_unmount(token) {
        Err(Failure::Retained(again)) => {
            assert_eq!(
                (again.stage, again.detached, again.token),
                (TeardownStage::Requests, true, token)
            );
        }
        other => panic!("{other:?}"),
    }
    let last = rig.harness.engine(helper);
    let end = client.diagnostics().unwrap();
    assert_eq!(last.namespaces, empty.namespaces + 1);
    assert_eq!(end.closed_namespaces, start.closed_namespaces);
    assert_eq!(rig.harness.serving.work().unwrap().mounts, 1);
    println!(
        "FP-21 retained: stage=Requests detached=true loops_joined=1 admitted_at_stop=1 mount_row=None phase=Retained activity=Closing engine_mount=Live cleanup=Live namespaces={}(baseline {}) owner_rows={} closed_namespaces={}->{} after_release(completed={}->{} phase=Retained lane_kept=1) store_consumer_half=NOT_STAGED",
        last.namespaces,
        empty.namespaces,
        last.owner_rows,
        start.closed_namespaces,
        end.closed_namespaces,
        parked.completed,
        after.completed
    );
    drop(guard);
    let closed = rig.harness.try_unmount(helper).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(helper));
    drop(closed);
    // The entry keeps its undrained lane, so the rig's own stop, which
    // requires no mount lane, does not apply: the owners are dropped here.
    let Rig {
        fixture,
        store,
        harness,
        ..
    } = rig;
    let mounted::Harness {
        owner,
        serving,
        service,
        ..
    } = harness;
    drop(service);
    drop(serving);
    owner.stop().unwrap();
    drop(store);
    fixture.cleanup();
}

const VICTIM: usize = 70_000;
const SEED: u8 = 7;

/// FP-29, the part R3 left: a READ that is being processed while a sibling
/// handle of the same file is released and the file's last name is removed.
/// After the leases are returned the read has exactly the file's bytes and
/// the kept descriptor reports link count 0, fetched from the daemon.
#[test]
fn fp29_a_parked_read_spans_the_release_of_a_sibling_handle_and_the_removal_of_its_name() {
    let rig = Rig::built("fp29-processing", |source| {
        fs::create_dir(source.join("cold")).unwrap();
        let path = source.join("cold/victim");
        fs::write(&path, pattern(SEED, VICTIM)).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        stamp_tree(source);
    });
    let helper = rig.harness.bind(1);
    let ready = rig.mount(2);
    let guard = Mounted(ready.directory.clone());
    let token = ready.token;
    // Before the hold, by an unregistered process of the command identity:
    // the base file is moved into a directory made through the mount and its
    // mode changed. Its name, parent and inode rows are local; its bytes are
    // still only in the base.
    passed(
        &bash_in(
            COMMAND,
            root(&ready),
            "set -euo pipefail; umask 022; mkdir local; mv -T cold/victim local/victim; chmod 0640 local/victim",
        ),
        "before the hold",
    );
    let path = root(&ready).join("local/victim");
    let named = fs::metadata(&path).unwrap();
    assert_eq!(
        (named.nlink(), named.len(), named.mode() & 0o777),
        (1, VICTIM as u64, 0o640)
    );
    let kept = Arc::new(
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECT)
            .open(&path)
            .unwrap(),
    );
    let sibling = File::open(&path).unwrap();
    quiet(&rig, token);
    let opened = rig.harness.engine(helper);
    let quiescent = work(&rig, token);
    let mut checks = Checks::default();
    let leases = holds::Leases::all(&rig.store, WAIT);
    let held = rig.store.read_work();

    // One READ with `O_DIRECT`: a foreground request served by the daemon.
    let (read_sender, read) = mpsc::channel();
    let reading = {
        let kept = kept.clone();
        thread::spawn(move || {
            let mut bytes = vec![0; 128 * 1024];
            let result = kept
                .read_at(&mut bytes, 0)
                .map(|length| bytes[..length].to_vec())
                .map_err(|error| error.raw_os_error());
            let _ = read_sender.send(result);
        })
    };
    within("the READ is parked on reader admission", || {
        work(&rig, token).parked == 1 && rig.store.read_work().waiting == 1
    });
    let parked = work(&rig, token);
    assert_eq!(
        (
            parked.handoffs - quiescent.handoffs,
            parked.admitted,
            parked.received
        ),
        (1, 1, 0),
        "{quiescent:?} -> {parked:?}"
    );

    // The sibling handle is released while the read is being processed.
    drop(sibling);
    let released = settled(&rig, token, |now| now.completed > parked.completed);
    let closed = rig.harness.engine(helper);
    checks.that(
        released.handoffs == parked.handoffs + 1
            && released.completed == parked.completed + 1
            && (released.admitted, released.parked) == (1, 1),
        || format!("FP-29: the sibling's RELEASE was not served beside the parked READ: {parked:?} -> {released:?}"),
    );

    // The last name is removed while the read is being processed.
    let (unlink_sender, unlink) = mpsc::channel();
    let unlinking = {
        let path = path.clone();
        thread::spawn(move || {
            let result = fs::remove_file(path).map_err(|error| error.raw_os_error());
            let _ = unlink_sender.send(result);
        })
    };
    let removed = unlink.recv_timeout(HELD).ok();
    let after = settled(&rig, token, |now| now.completed > released.completed);
    let admission = rig.store.read_work();
    let unread = matches!(read.try_recv(), Err(mpsc::TryRecvError::Empty));
    println!(
        "FP-29 hold: leases_held={} read(handoffs={}->{} parked=1 read_admissions_waiting=1) sibling_release(handoffs={}->{} completed={}->{}) engine_owner_rows(two_handles_quiescent={} one_handle_and_parked_read={}) unlink_while_held={removed:?} after_unlink(handoffs={} completed={} parked={} admitted={}) read_still_parked={unread} read_admissions_waiting={} reader_grants={}->{}",
        leases.count(),
        quiescent.handoffs,
        parked.handoffs,
        parked.handoffs,
        released.handoffs,
        parked.completed,
        released.completed,
        opened.owner_rows,
        closed.owner_rows,
        after.handoffs,
        after.completed,
        after.parked,
        after.admitted,
        admission.waiting,
        held.grants,
        admission.grants
    );
    checks.that(removed == Some(Ok(())), || {
        format!("FP-29: removing the name did not complete while every Store reader was leased: {removed:?}, {after:?}, {admission:?}")
    });
    checks.that(
        unread && after.parked == 1 && admission.waiting == 1 && admission.grants == held.grants,
        || format!("FP-29: the READ did not stay parked across the release and the removal: read_still_parked={unread} {after:?} {admission:?}"),
    );

    leases.release();
    let bytes = read
        .recv_timeout(WAIT)
        .expect("the READ did not return after the leases were returned");
    reading.join().unwrap();
    let removed = match removed {
        Some(result) => result,
        None => unlink
            .recv_timeout(WAIT)
            .expect("the removal did not return after the leases were returned"),
    };
    unlinking.join().unwrap();
    assert_eq!(removed, Ok(()), "the removal's own result");
    let bytes = bytes.unwrap_or_else(|errno| panic!("the spanning READ failed: {errno:?}"));
    assert!(
        bytes == pattern(SEED, VICTIM),
        "the spanning READ returned {} bytes that are not the file's",
        bytes.len()
    );
    let seen = forced(&kept).unwrap();
    assert_eq!(
        seen,
        (named.ino(), 0, VICTIM as u64),
        "the kept descriptor: inode, link count 0, size"
    );
    assert_eq!(
        fs::symlink_metadata(&path)
            .unwrap_err()
            .raw_os_error()
            .map(Errno::from_raw),
        Some(Errno::ENOENT)
    );
    // The nameless inode is still read exactly through the kept descriptor.
    let mut again = vec![0; 128 * 1024];
    let length = kept.read_at(&mut again, 0).unwrap();
    assert!(again[..length] == pattern(SEED, VICTIM)[..]);
    quiet(&rig, token);
    let last = work(&rig, token);
    assert_eq!((last.retained, last.terminal, last.unadmitted), (0, 0, 0));
    drop(kept);
    quiet(&rig, token);
    let receipt = rig.unmount(&ready);
    let counts = opcodes(&receipt);
    println!(
        "FP-29 after release: read_bytes_exact={} kept_descriptor(inode={} links={} size={}) name=ENOENT second_read_exact=true receipt_opcodes(read={} release={} unlink={} rename={} setattr={} getattr={})",
        bytes.len(),
        seen.0,
        seen.1,
        seen.2,
        counts[Opcode::Read as usize],
        counts[Opcode::Release as usize],
        counts[Opcode::Unlink as usize],
        counts[Opcode::Rename as usize],
        counts[Opcode::Setattr as usize],
        counts[Opcode::Getattr as usize]
    );
    drop(guard);
    let closed = rig.harness.try_unmount(helper).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(helper));
    drop(closed);
    rig.finish();
    checks.done("FP-29");
}

/// Reclaim of the test container's own memory cgroup, and nothing wider.
struct Reclaim(PathBuf);
impl Reclaim {
    /// Declared precondition, not a product effect: this container's cgroup
    /// interface must be writable so reclaim can be requested for it. The
    /// file is the `memory.reclaim` of the cgroup this process is a member
    /// of, read from its own membership record.
    fn own() -> Self {
        let membership = fs::read_to_string("/proc/self/cgroup").unwrap();
        let relative = membership
            .lines()
            .find_map(|line| line.strip_prefix("0::"))
            .unwrap_or_else(|| panic!("precondition: a cgroup v2 membership: {membership}"));
        let file = Path::new("/sys/fs/cgroup")
            .join(relative.trim_start_matches('/'))
            .join("memory.reclaim");
        let remount = Process::new("mount")
            .args(["-o", "remount,rw", "/sys/fs/cgroup"])
            .output()
            .unwrap();
        assert!(
            remount.status.success() && file.exists(),
            "precondition: container-scoped {} is required: {remount:?}",
            file.display()
        );
        println!(
            "FP-31 reclaim scope: membership={:?} file={}",
            membership.trim(),
            file.display()
        );
        Self(file)
    }
    /// One reclaim request to the kernel, for more than the cgroup can give
    /// back so that its dentry and inode lists are walked to the end. The
    /// kernel's own answer to the request is not a product result.
    fn request(&self) {
        let _ = OpenOptions::new()
            .write(true)
            .open(&self.0)
            .and_then(|mut file| file.write_all(b"1G"));
    }
}
/// Reclaim requests, one per observation, until this mount is idle and the
/// engine's daemon-wide owner rows are `rows`; returns the requests made.
/// When the bound expires the counters as they are go into the failure.
fn returned(
    reclaim: &Reclaim,
    rig: &Rig,
    token: WorkspaceToken,
    helper: WorkspaceToken,
    rows: u64,
    what: &str,
) -> u32 {
    let deadline = Instant::now() + RECLAIM;
    let mut requests = 0;
    loop {
        requests += 1;
        reclaim.request();
        let (now, counts) = (work(rig, token), rig.harness.engine(helper));
        if now.received == 0 && now.admitted == 0 && counts.owner_rows == rows {
            return requests;
        }
        assert!(
            Instant::now() < deadline,
            "bounded observation expired: {what}: reclaim_requests={requests} owner_rows={} wanted={rows} {now:?} {counts:?}",
            counts.owner_rows
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn path_only(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH)
        .open(path)
        .unwrap()
}

/// FP-31, a partial decrement. One name is looked up (count 1) and its inode
/// evicted while both Lifecycle credits are held, so the FORGET unit of 1 is
/// parked before its job. The name is then looked up again (count 2). When
/// the credits return, the parked unit leaves the row with the remainder: the
/// row is still there, and one further unit of exactly 1 returns it with no
/// underflow retained.
#[test]
fn fp31_a_forget_unit_smaller_than_the_held_count_leaves_the_exact_remainder() {
    let reclaim = Reclaim::own();
    let rig = Rig::built("fp31-partial", |source| {
        fs::write(source.join("single"), b"single\n").unwrap();
        stamp_tree(source);
    });
    let client = rig.harness.owner.client();
    let helper = rig.harness.bind(1);
    let ready = rig.mount(2);
    let guard = Mounted(ready.directory.clone());
    let token = ready.token;
    let route = holds::route(&rig.harness.service, token);
    quiet(&rig, token);
    let baseline = rig.harness.engine(helper);
    assert_eq!(work(&rig, token).forget_units, 0);
    let path = root(&ready).join("single");
    let inode = fs::symlink_metadata(&path).unwrap().ino();
    quiet(&rig, token);
    let looked = rig.harness.engine(helper);
    assert_eq!(
        looked.owner_rows - baseline.owner_rows,
        1,
        "one indexed lookup owner for the one kernel inode"
    );
    let mut checks = Checks::default();
    let credits = holds::Credits::lifecycle(&client, route, 2, WAIT);

    // The kernel evicts the inode: one FORGET unit of 1, parked.
    let mut first_requests = 0;
    before("the FORGET unit is parked before its job", RECLAIM, || {
        first_requests += 1;
        reclaim.request();
        let now = work(&rig, token);
        now.forget_units == 1 && now.parked == 1
    });
    let parked = work(&rig, token);
    assert_eq!((parked.admitted, parked.received), (1, 0), "{parked:?}");
    assert_eq!(
        rig.harness.engine(helper).owner_rows,
        looked.owner_rows,
        "nothing is decremented while the unit is parked"
    );

    // A second LOOKUP of the same name: the count becomes 2.
    let (sender, receiver) = mpsc::channel();
    let looking = {
        let path = path.clone();
        thread::spawn(move || {
            let result = fs::symlink_metadata(path)
                .map(|metadata| metadata.ino())
                .map_err(|error| error.raw_os_error());
            let _ = sender.send(result);
        })
    };
    let again = receiver.recv_timeout(HELD).ok();
    let second = settled(&rig, token, |now| now.completed == parked.completed + 1);
    let both = rig.harness.engine(helper);
    println!(
        "FP-31 partial hold: reclaim_requests={first_requests} forget_units={} parked_forget=1 second_lookup={again:?} handoffs={}->{} parked={} admitted={} owner_rows(baseline={} looked={} while_parked={})",
        parked.forget_units,
        parked.handoffs,
        second.handoffs,
        second.parked,
        second.admitted,
        baseline.owner_rows,
        looked.owner_rows,
        both.owner_rows
    );
    checks.that(again == Some(Ok(inode)), || {
        format!("FP-31: the second LOOKUP did not return the same inode while the FORGET was parked: {again:?} {second:?}")
    });
    // The LOOKUP is one owner visit that records no request source: it
    // completed whole, with no release left to park beside the FORGET unit
    // and no processing owner row beside the one lookup row.
    checks.that(
        second.handoffs == parked.handoffs + 1
            && second.forget_units == 1
            && (second.parked, second.admitted) == (1, 1)
            && second.completed == parked.completed + 1
            && both.owner_rows == looked.owner_rows,
        || format!("FP-31: not exactly one more request, completed, beside the parked unit: {parked:?} -> {second:?}, {both:?}"),
    );

    credits.release();
    if again.is_none() {
        let late = receiver.recv_timeout(WAIT);
        println!("FP-31 partial: the second LOOKUP after the credits returned: {late:?}");
    }
    looking.join().unwrap();
    quiet(&rig, token);
    let after = work(&rig, token);
    let partial = rig.harness.engine(helper);
    assert_eq!(after.forget_units, 1);
    assert_eq!(
        (after.retained, after.unadmitted, after.terminal),
        (0, 0, 0),
        "{after:?}"
    );
    checks.that(partial.owner_rows == looked.owner_rows, || {
        format!(
            "FP-31: a unit of 1 against a count of 2 did not leave the lookup row: owner_rows {} (looked {}, baseline {})",
            partial.owner_rows, looked.owner_rows, baseline.owner_rows
        )
    });

    // The second eviction returns the remainder of exactly 1.
    let second_requests = returned(
        &reclaim,
        &rig,
        token,
        helper,
        baseline.owner_rows,
        "the remaining reference is returned",
    );
    quiet(&rig, token);
    // One more request finds nothing left to return.
    reclaim.request();
    quiet(&rig, token);
    let last = work(&rig, token);
    let returned = rig.harness.engine(helper);
    assert_eq!(last.forget_units, 2, "two units of one each");
    assert_eq!(
        (last.retained, last.unadmitted, last.terminal),
        (0, 0, 0),
        "an underflow would be retained: {last:?}"
    );
    assert_eq!(returned.owner_rows, baseline.owner_rows);
    assert_eq!(rig.harness.phase(token), NativePhase::Ready);
    let receipt = rig.unmount(&ready);
    let counts = opcodes(&receipt);
    println!(
        "FP-31 partial: count 1 -> 2 (second lookup) -> 1 (unit of 1, row kept: owner_rows={}) -> 0 (unit of 1, row returned: owner_rows={}) forget_units={} reclaim_requests={first_requests}+{second_requests}+1 retained=0 receipt_opcodes(lookup={} forget_frames={} batch_frames={})",
        partial.owner_rows,
        returned.owner_rows,
        last.forget_units,
        counts[Opcode::Lookup as usize],
        counts[Opcode::Forget as usize],
        counts[Opcode::BatchForget as usize]
    );
    assert_eq!(counter(&receipt, "forget_units"), 2, "{receipt}");
    drop(guard);
    let closed = rig.harness.try_unmount(helper).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(helper));
    drop(closed);
    rig.finish();
    checks.done("FP-31 partial decrement");
}

const DIRECTORIES: usize = 20;
const FILES: usize = 100;
/// `tree`, its directories and their files; the alias is a second name.
const INODES: u64 = (1 + DIRECTORIES + DIRECTORIES * FILES) as u64;
const NAMES: u64 = INODES + 1;
/// Files of the last directory kept referenced through the first reclaim.
const PINNED: usize = 50;
/// What the pins keep: themselves, their directory and `tree`.
const KEPT: u64 = PINNED as u64 + 2;
/// Directories whose names are looked up again before the detach.
const AGAIN: usize = 10;
/// Lookup rows outstanding at the detach: `tree`, those directories and
/// their files.
const OUTSTANDING: u64 = (1 + AGAIN + AGAIN * FILES) as u64;

fn directory(d: usize) -> String {
    format!("tree/d-{d:02}")
}
fn file(d: usize, f: usize) -> String {
    format!("tree/d-{d:02}/f-{f:03}")
}
/// One `lstat` of `tree`, of each of the first `directories` directories and
/// of each of their files, and of the alias: names looked up and the
/// distinct inodes they bound.
fn walk(mount: &Path, directories: usize) -> (u64, BTreeSet<u64>) {
    let mut inodes = BTreeSet::new();
    let mut names = 0;
    let mut look = |relative: &str| {
        names += 1;
        inodes.insert(fs::symlink_metadata(mount.join(relative)).unwrap().ino());
    };
    look("tree");
    for d in 0..directories {
        look(&directory(d));
        for f in 0..FILES {
            look(&file(d, f));
        }
    }
    look("tree/d-00/alias");
    (names, inodes)
}
/// Owner maintenance counters whenever they change, on a second thread that
/// ends when stopped, dropped or after a minute.
struct Turns {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Vec<(u64, u64)>>>,
}
impl Turns {
    fn start(client: OwnerClient) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(60);
            let mut seen: Vec<(u64, u64)> = Vec::new();
            while !stopped.load(Ordering::SeqCst) && Instant::now() < deadline {
                if let Ok(work) = client.diagnostics() {
                    let now = (work.maintenance_jobs, work.maintenance_rows);
                    if seen.last() != Some(&now) {
                        seen.push(now);
                    }
                }
                thread::yield_now();
            }
            seen
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
    fn finish(mut self) -> Vec<(u64, u64)> {
        self.stop.store(true, Ordering::SeqCst);
        let thread = self.thread.take().unwrap();
        within("the sampling thread ended", || thread.is_finished());
        thread.join().unwrap()
    }
}
impl Drop for Turns {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// FP-31, mounted halves at about two thousand lookups. Reclaim with fifty
/// files kept referenced returns exactly the other inodes, one FORGET unit
/// each (the hard-linked inode's two references in one unit), in batched
/// frames; releasing the references returns the rest. A detach with 1,011
/// lookups outstanding then retires them in maintenance turns of at most 64
/// rows with no full-scan step in the retirement's statement family, until
/// the namespace is gone and the engine's counts are what they were.
#[test]
fn fp31_batched_forget_is_exact_and_outstanding_lookups_retire_in_bounded_turns_after_detach() {
    let reclaim = Reclaim::own();
    let rig = Rig::built("fp31-wide", |source| {
        for d in 0..DIRECTORIES {
            fs::create_dir_all(source.join(directory(d))).unwrap();
            for f in 0..FILES {
                fs::write(source.join(file(d, f)), format!("{d:02}/{f:03}\n")).unwrap();
            }
        }
        fs::hard_link(source.join(file(0, 0)), source.join("tree/d-00/alias")).unwrap();
        stamp_tree(source);
    });
    let client = rig.harness.owner.client();
    let helper = rig.harness.bind(1);
    within("the owner is idle after the bind", || {
        client.diagnostics().unwrap().outstanding == 0
    });
    let empty = rig.harness.engine(helper);
    let ready = rig.mount(2);
    let guard = Mounted(ready.directory.clone());
    let token = ready.token;
    let route = holds::route(&rig.harness.service, token);
    let mount = root(&ready).to_owned();
    quiet(&rig, token);
    let baseline = rig.harness.engine(helper);
    assert_eq!(work(&rig, token).forget_units, 0);

    // References with kernel lookup custody only, then every name once.
    let pins: Vec<File> = (0..PINNED)
        .map(|f| path_only(&mount.join(file(DIRECTORIES - 1, f))))
        .collect();
    let (names, inodes) = walk(&mount, DIRECTORIES);
    assert_eq!((names, inodes.len() as u64), (NAMES, INODES));
    quiet(&rig, token);
    let looked = rig.harness.engine(helper);
    assert_eq!(
        looked.owner_rows - baseline.owner_rows,
        INODES,
        "one indexed lookup owner per live kernel inode: {baseline:?} {looked:?}"
    );

    // Partial return: everything but what the pins keep.
    let first_requests = returned(
        &reclaim,
        &rig,
        token,
        helper,
        baseline.owner_rows + KEPT,
        "every unreferenced inode returned by FORGET",
    );
    quiet(&rig, token);
    reclaim.request();
    quiet(&rig, token);
    let partial = work(&rig, token);
    let kept = rig.harness.engine(helper);
    println!(
        "FP-31 wide: names={names} inodes={INODES} pinned_files={PINNED} reclaim_requests={first_requests}+1 forget_units={} owner_rows(baseline={} looked={} after_reclaim={}) engine_decrement={}",
        partial.forget_units,
        baseline.owner_rows,
        looked.owner_rows,
        kept.owner_rows,
        looked.owner_rows - kept.owner_rows
    );
    assert_eq!(
        partial.forget_units,
        INODES - KEPT,
        "one unit per evicted inode"
    );
    assert_eq!(
        looked.owner_rows - kept.owner_rows,
        partial.forget_units,
        "the engine's decrement is the units received"
    );
    assert_eq!(kept.owner_rows - baseline.owner_rows, KEPT);
    assert_eq!(
        (partial.retained, partial.unadmitted, partial.terminal),
        (0, 0, 0),
        "{partial:?}"
    );

    // The rest, once nothing references it.
    drop(pins);
    let second_requests = returned(
        &reclaim,
        &rig,
        token,
        helper,
        baseline.owner_rows,
        "every lookup owner returned by FORGET",
    );
    quiet(&rig, token);
    let all = work(&rig, token);
    assert_eq!(all.forget_units, INODES, "one unit per inode in all");
    assert_eq!((all.retained, all.unadmitted, all.terminal), (0, 0, 0));
    assert_eq!(rig.harness.phase(token), NativePhase::Ready);

    // Lookups still outstanding at the detach.
    let (again_names, again_inodes) = walk(&mount, AGAIN);
    assert_eq!(again_inodes.len() as u64, OUTSTANDING);
    assert!(again_inodes.is_subset(&inodes), "the same inodes again");
    quiet(&rig, token);
    let outstanding = rig.harness.engine(helper);
    assert_eq!(
        outstanding.owner_rows - baseline.owner_rows,
        OUTSTANDING,
        "reacquired after FORGET"
    );
    let start = client.diagnostics().unwrap();
    let turns = Turns::start(client.clone());
    let receipt = rig.unmount(&ready);
    let mut states = Vec::new();
    before("cleanup state Gone", LONG, || {
        let state = cleanup(&client, route);
        if states.last() != Some(&state) {
            states.push(state);
        }
        state == CleanupState::Gone
    });
    let mut last = rig.harness.engine(helper);
    before("engine counts returned to their baseline", LONG, || {
        last = rig.harness.engine(helper);
        last == empty
    });
    let seen = turns.finish();
    let end = client.diagnostics().unwrap();

    // The receipt: FORGET frames, batched frames and units of the connection.
    let counts = opcodes(&receipt);
    let (frames, batched) = (
        counts[Opcode::Forget as usize],
        counts[Opcode::BatchForget as usize],
    );
    let units = counter(&receipt, "forget_units");
    println!(
        "FP-31 wide: second_reclaim_requests={second_requests} forget_units_total={units} forget_frames={frames} frames_with_several_units={batched} lookup_frames={} unadmitted={} outstanding_at_detach(names={again_names} rows={OUTSTANDING})",
        counts[Opcode::Lookup as usize],
        counter(&receipt, "unadmitted")
    );
    assert_eq!(units, INODES, "no unit arrived after the detach: {receipt}");
    assert!(
        batched >= 1 && frames < units,
        "batched delivery: {frames} frames, {batched} with several units, {units} units"
    );

    // Retirement of the outstanding lookups, by the owner's own counters.
    let (jobs, rows) = (
        end.maintenance_jobs - start.maintenance_jobs,
        end.maintenance_rows - start.maintenance_rows,
    );
    let mut previous = (start.maintenance_jobs, start.maintenance_rows);
    let (mut single, mut largest, mut over) = (0, 0, Vec::new());
    for now in seen.iter().copied() {
        if now.0 < previous.0 {
            continue;
        }
        let (turns, retired) = (now.0 - previous.0, now.1 - previous.1);
        if retired > WINDOW * turns && over.len() < 8 {
            over.push((previous, now));
        }
        if turns == 1 {
            single += 1;
            largest = largest.max(retired);
        }
        previous = now;
    }
    let lease = |work: &OwnerWork| work.sql_maintenance.statements[StatementKind::Lease as usize];
    let scans = |work: &OwnerWork| -> u64 {
        work.sql_maintenance
            .statements
            .iter()
            .map(|statement| statement.fullscan_steps)
            .sum()
    };
    let (lease_start, lease_end) = (lease(&start), lease(&end));
    println!(
        "FP-31 retire: outstanding_rows={OUTSTANDING} maintenance_jobs={jobs} maintenance_rows={rows} samples={} single_turn_samples={single} largest_single_turn_rows={largest} intervals_over_{WINDOW}_per_turn={} lease_statements(executions={} rows_returned={} rows_changed={} vm_steps={} fullscan_steps={}) all_maintenance_fullscan_steps={} cleanup_states={states:?} closed_namespaces={}->{} counts_equal_baseline=true",
        seen.len(),
        over.len(),
        lease_end.executions - lease_start.executions,
        lease_end.rows_returned - lease_start.rows_returned,
        lease_end.rows_changed - lease_start.rows_changed,
        lease_end.vm_steps - lease_start.vm_steps,
        lease_end.fullscan_steps - lease_start.fullscan_steps,
        scans(&end) - scans(&start),
        start.closed_namespaces,
        end.closed_namespaces
    );
    assert!(rows >= OUTSTANDING, "{rows} rows for {OUTSTANDING} lookups");
    assert!(
        jobs >= OUTSTANDING.div_ceil(WINDOW),
        "{jobs} turns for {OUTSTANDING} lookups"
    );
    assert!(
        rows <= WINDOW * jobs,
        "{rows} rows in {jobs} turns exceeds {WINDOW} per turn"
    );
    assert!(
        over.is_empty(),
        "more than {WINDOW} rows per turn: {over:?}"
    );
    assert!(
        largest <= WINDOW && single > 0,
        "{single} single turns, largest {largest}"
    );
    assert_eq!(
        lease_end.fullscan_steps - lease_start.fullscan_steps,
        0,
        "no scan in the retirement's statements"
    );
    assert_eq!(end.closed_namespaces, start.closed_namespaces + 1);
    assert!(client.maintenance_failure().unwrap().is_none());
    assert_eq!(last, empty);
    drop(guard);
    let closed = rig.harness.try_unmount(helper).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(helper));
    drop(closed);
    rig.finish();
}
