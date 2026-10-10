//! FP-31, underflow and failure custody on a REAL kernel FORGET (R8b track
//! T-J). Specification section 6: "FORGET subtracts the kernel's exact
//! `nlookup`, checks the mount/incarnation and underflow, and releases the
//! lookup lease only at zero. ... A decrement failure preserves the record
//! and custody and stops that cleanup; it is never converted to a successful
//! counter update or automatically retried."
//!
//! The kernel alone never forgets more than it was replied. The adversarial
//! state is therefore made through the public owner port, and the request is
//! the kernel's own: on a live mount a name is looked up, the engine's lookup
//! count is reduced by `NativeJob::Forget` through the owner client, and the
//! kernel's FORGET is then asked for by reclaim of the test container's own
//! memory cgroup (`memory.reclaim`; never a system-wide cache drop).
//!
//! - **Row absent.** One lookup, count 1, reduced by 1: the row is gone, and
//!   the kernel's unit of 1 finds none (`OverlayError::Stale`).
//! - **Row too small, the literal underflow branch.** One inode with two
//!   names, each looked up once: count 2, reduced by 1. The row is present
//!   with count 1, and the kernel's one unit of 2 underflows it
//!   (`layerfs-overlay` `lifetime/native.rs`, `checked_sub`).
//!
//! Two scopes, because of what each can observe:
//!
//! - The first two tests use the control Service, as every mounted proof
//!   does: the unit is retained, the engine's counts are unchanged by the
//!   failed job, no decrement is guessed, and the later Unmount stops
//!   `Retained` at `Requests` (`layerfs-fuse` `session/drain.rs`) and repeats
//!   that custody. The daemon exposes no observation of a retained request's
//!   original failure (`MountQueue::inspect_retained` has no caller in daemon
//!   source), so the unit's original input cannot be read at this scope.
//! - The three session tests attach the same real kernel mount through the
//!   public pieces the Service composes (`Dispatch`, `NativeSession::attach`
//!   over the bound Workspace's own request services), keep the lane handle
//!   and read each retained unit's original input and original cause through
//!   `MountQueue::inspect_retained`. Each then makes the session's own detach
//!   and drain, which stop at `Requests` and stop there again when repeated.
//!   One test stages each inconsistency alone; the third stages both on one
//!   mount, where a unit received behind the fence of the other is
//!   unadmitted and no decrement is attempted for it.
//!
//! Attempt 1 of this file failed in the test's own bookkeeping, not the
//! product: a Status is itself one owner job (`control/status.rs`), and one
//! retained unit fences its lane, so two inodes on one mount do not give
//! two retained units.
//!
//! Caveat, stated as the audit stated it: this proves custody of a native
//! request against engine state made inconsistent through the engine's own
//! public port. It is not a misbehaving kernel.
//!
//! NOT in scope here (plan section 6): a FORGET of a foreign incarnation.
//! The incarnation is the lane's own identity, never kernel input
//! (`layerfs-fuse` `request/inline.rs`).
#![cfg(target_os = "linux")]
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
    Activity, NativePhase, NativeWork, Reply, Request, TeardownStage, WorkspaceToken,
};
use layerfs_daemon::{
    bootstrap::open_store, control::Failure, store::BindRequest, Command, Completion, NativeJob,
    NativeReply, Owner, OwnerClient, OwnerConfig, OwnerError, Response,
};
use layerfs_fuse::{
    attributes::Identity,
    session::{Detach, DrainStage, NativeSession, SessionConfig, Undrained},
    Dispatch, DispatchConfig, DispatchError, FailureView, MountQueue, HANDOFFS,
};
use layerfs_history::WorkspaceId;
use layerfs_overlay::{
    CleanupState, NativeMount, NativeMountState, ProfileConfig, Route, StoredCounts,
};
use layerfs_storage::ReservationBlocks;
use mounted::{mount_entry, Harness, COMMAND};
use nix::unistd::{Gid, Uid};
use rig::{root, stamp_tree, Rig};
use std::{
    fmt::Debug,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command as Process,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

/// Every bounded readiness or completion observation of the tests.
const WAIT: Duration = Duration::from_secs(5);
/// Bound of one reclaim-until-delivered observation; how many requests the
/// kernel needs before it evicts an inode is its own business.
const RECLAIM: Duration = Duration::from_secs(30);

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
/// Never leaves a kernel mount behind: if the test unwinds, one lazy
/// umount(8) launched and reaped by the test detaches what is still there.
struct Mounted(String);
impl Drop for Mounted {
    fn drop(&mut self) {
        if mount_entry(&self.0).is_some() {
            let done = Process::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "FP-31 GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}
/// Product expectations that did not hold, reported after the mount is torn
/// down, so a deviation leaves its whole evidence.
#[derive(Default)]
struct Checks(Vec<String>);
impl Checks {
    fn that(&mut self, holds: bool, what: impl FnOnce() -> String) {
        if !holds {
            let what = what();
            println!("FP-31 DEVIATION {what}");
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
                    "owner job {back:?}: no free slot"
                );
                command = back;
                thread::sleep(Duration::from_millis(1));
            }
            Err((error, back)) => panic!("owner job {back:?}: {error:?}"),
        }
    };
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < deadline, "owner job incomplete");
        thread::sleep(Duration::from_millis(1));
    }
}
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
fn cleanup(client: &OwnerClient, route: Route) -> CleanupState {
    let done = job(client, route, Command::CleanupState);
    match done.result() {
        Ok(Response::CleanupState(state)) => *state,
        other => panic!("{other:?}"),
    }
}
/// One lookup decrement through the public owner port, as
/// `native_jobs.rs` submits it: the engine's original answer as text, `Ok`
/// when the job succeeded.
fn decrement(
    client: &OwnerClient,
    route: Route,
    mount: NativeMount,
    serial: u64,
    count: u64,
) -> Result<(), String> {
    let done = job(
        client,
        route,
        Command::Native(NativeJob::Forget {
            mount,
            serial,
            count,
        }),
    );
    match done.result() {
        Ok(Response::Native(NativeReply::Done)) => Ok(()),
        other => Err(format!("{other:?}")),
    }
}
/// The engine serial of a kernel inode other than the root: the reversible
/// offset of `layerfs-fuse` `attributes.rs` (`Identity::serial`).
fn serial(inode: u64) -> u64 {
    assert!(inode > 1, "not the root inode");
    inode - 1
}

/// Which inconsistency the owner port makes before the kernel's FORGET.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Reduced {
    /// Count 1, reduced by 1: no row is left for the kernel's unit of 1.
    Absent,
    /// Count 2 through two names, reduced by 1: the kernel's unit of 2
    /// meets a row with count 1.
    TooSmall,
}

fn work(harness: &Harness, token: WorkspaceToken) -> NativeWork {
    harness.status(token).native.unwrap().work.unwrap()
}
/// Two consecutive observations with nothing received or admitted and no
/// request completed in between. A readiness wait before one attempt.
fn quiet(harness: &Harness, token: WorkspaceToken) {
    let mut last = None;
    within("connection quiescent", || {
        let now = work(harness, token);
        let settled = now.received == 0 && now.admitted == 0 && last == Some(now.completed);
        last = Some(now.completed);
        thread::sleep(Duration::from_millis(10));
        settled
    });
}

/// The control-Service scope: custody of the refused unit by counts, and
/// the later Unmount.
fn refused_at_the_service(reduced: Reduced) {
    let name = format!("FP-31 {reduced:?}");
    let reclaim = Reclaim::own();
    let rig = Rig::built(
        match reduced {
            Reduced::Absent => "fp31-absent",
            Reduced::TooSmall => "fp31-small",
        },
        move |source| {
            fs::write(source.join("single"), b"single\n").unwrap();
            if reduced == Reduced::TooSmall {
                fs::hard_link(source.join("single"), source.join("alias")).unwrap();
            }
            stamp_tree(source);
        },
    );
    let h = &rig.harness;
    let client = h.owner.client();
    let helper = h.bind(1);
    within("the owner is idle after the bind", || {
        client.diagnostics().unwrap().outstanding == 0
    });
    let empty = h.engine(helper);
    let start = client.diagnostics().unwrap();
    let ready = rig.mount(2);
    let guard = Mounted(ready.directory.clone());
    let token = ready.token;
    let route = h.service.operation(token).unwrap().workspace().route();
    let mut checks = Checks::default();
    quiet(h, token);
    let baseline = h.engine(helper);
    assert_eq!(work(h, token).forget_units, 0);

    // One LOOKUP per name: the kernel's count of the inode.
    let inode = fs::symlink_metadata(root(&ready).join("single"))
        .unwrap()
        .ino();
    let held = match reduced {
        Reduced::Absent => 1,
        Reduced::TooSmall => {
            let alias = fs::symlink_metadata(root(&ready).join("alias")).unwrap();
            assert_eq!(
                (alias.ino(), alias.nlink()),
                (inode, 2),
                "one inode, two names"
            );
            2
        }
    };
    quiet(h, token);
    let looked = h.engine(helper);
    // The inode's lookup row and its file-custody row.
    assert_eq!(
        looked.owner_details - baseline.owner_details,
        2,
        "{baseline:?} {looked:?}"
    );
    let mount = engine_mount(&client, route).expect("attached engine mount");

    // The engine's count is reduced by one through the public owner port.
    assert_eq!(decrement(&client, route, mount, serial(inode), 1), Ok(()));
    let stolen = h.engine(helper);
    match reduced {
        Reduced::Absent => assert_eq!(
            stolen.owner_details, baseline.owner_details,
            "count 1 reduced by 1: the row and its custody are gone"
        ),
        Reduced::TooSmall => assert_eq!(
            stolen.owner_details, looked.owner_details,
            "count 2 reduced by 1: the row is kept"
        ),
    }
    quiet(h, token);
    let staged = work(h, token);
    let jobs = client.diagnostics().unwrap().admitted;
    println!(
        "{name} staged: inode={inode} serial={} kernel_count={held} engine_reduced_by=1 owner_details(baseline={} looked={} reduced={}) engine_mount={mount:?}",
        serial(inode),
        baseline.owner_details,
        looked.owner_details,
        stolen.owner_details
    );

    // The kernel's own FORGET of the inode, delivered and disposed. Every
    // Status and every count observation of the test is itself one owner
    // job (`control/status.rs`, `Command::State`; `Command::Resources`), so
    // they are counted and subtracted.
    let (mut requests, mut observations) = (0, 0);
    before("the kernel's FORGET unit was delivered", RECLAIM, || {
        requests += 1;
        observations += 1;
        reclaim.request();
        let now = work(h, token);
        now.forget_units == 1 && now.received == 0 && now.admitted == now.retained
    });
    let refused = work(h, token);
    let after = h.engine(helper);
    observations += 2;
    let submitted = client.diagnostics().unwrap().admitted - jobs - observations;
    println!(
        "{name}: reclaim_requests={requests} native work {staged:?} -> {refused:?} owner_jobs_since_staged={submitted} (beside the test's {observations} observations) engine_counts_equal_reduced={}",
        after == stolen
    );
    // Refused and retained: the unit stays admitted and uncompleted.
    checks.that(
        (refused.forget_units, refused.retained, refused.admitted) == (1, 1, 1)
            && (refused.unadmitted, refused.terminal) == (0, 0)
            && refused.completed == staged.completed
            && refused.handoffs == staged.handoffs + 1,
        || format!("{name}: the kernel's unit was not retained: {staged:?} -> {refused:?}"),
    );
    // Exactly one decrement job was attempted for it, never a second.
    checks.that(submitted == 1, || {
        format!("{name}: {submitted} owner jobs since the staging beside the test's own, not the unit's one")
    });
    // The failed job changed nothing.
    checks.that(after == stolen, || {
        format!("{name}: the refused decrement changed the engine: {stolen:?} -> {after:?}")
    });
    // No decrement was guessed: the row is exactly as the owner port left it.
    match reduced {
        Reduced::Absent => {
            let again = decrement(&client, route, mount, serial(inode), 1);
            println!("{name}: owner-port decrement of the absent row: {again:?}");
            checks.that(
                again.as_ref().is_err_and(|error| error.contains("Stale")),
                || format!("{name}: a lookup row exists after the refused unit: {again:?}"),
            );
        }
        Reduced::TooSmall => {
            let one = decrement(&client, route, mount, serial(inode), 1);
            let counts = h.engine(helper);
            let none = decrement(&client, route, mount, serial(inode), 1);
            println!(
                "{name}: owner-port decrements of the kept row: first={one:?} owner_details={} second={none:?}",
                counts.owner_details
            );
            checks.that(
                one == Ok(()) && counts.owner_details == baseline.owner_details,
                || format!("{name}: the kept row did not hold exactly 1: {one:?} {counts:?}"),
            );
            checks.that(
                none.as_ref().is_err_and(|error| error.contains("Stale")),
                || format!("{name}: the row held more than 1: {none:?}"),
            );
        }
    }
    let status = h.status(token);
    println!(
        "{name}: before the Unmount: activity={:?} phase={:?}",
        status.activity,
        status.native.as_ref().map(|native| native.phase)
    );

    // The later Unmount: detach, then the drain finds the retained unit.
    let later = client.diagnostics().unwrap().admitted;
    let custody = match h.try_unmount(token) {
        Err(Failure::Retained(custody)) => *custody,
        Ok(done) => panic!(
            "{name}: unmounted with a retained FORGET unit: {:?}",
            done.reply
        ),
        Err(other) => panic!("{name}: {}", brief(&other)),
    };
    let unmounted = client.diagnostics().unwrap().admitted;
    println!(
        "{name}: owner_jobs_admitted_by_the_unmount={} custody={custody:?}",
        unmounted - later
    );
    assert_eq!(unmounted, later, "the Unmount submitted no owner job");
    assert_eq!(custody.token, token);
    assert_eq!(custody.stage, TeardownStage::Requests, "{custody:?}");
    assert!(custody.detached, "the kernel detach is known");
    assert!(custody.forced.is_none());
    let stopped = custody.work.expect("counters at the stopping boundary");
    assert_eq!(
        (stopped.loops_configured, stopped.loops_joined),
        (1, 1),
        "{stopped:?}"
    );
    assert_eq!(
        (
            stopped.received,
            stopped.admitted,
            stopped.retained,
            stopped.forget_units,
            stopped.unadmitted
        ),
        (0, 1, 1, 1, 0),
        "{stopped:?}"
    );
    assert!(mount_entry(&ready.directory).is_none());
    let status = h.status(token);
    assert_eq!(status.activity, Activity::Closing);
    let native = status.native.unwrap();
    assert_eq!(native.phase, NativePhase::Retained);
    assert!(native.detached);
    assert_eq!(native.ready.as_ref(), Some(&ready));
    let kept = h
        .service
        .retained_native(token)
        .unwrap()
        .expect("retained owner");
    assert!(kept.contains("stage: Requests"), "{kept}");
    // Later operations answer with the same custody and attempt nothing.
    let later = client.diagnostics().unwrap().admitted;
    for request in [Request::Unmount(token), Request::Attach(token)] {
        match h.service.execute_control(&request) {
            Err(Failure::Retained(again)) => assert_eq!(*again, custody, "{request:?}"),
            other => panic!("{name}: {request:?}: {}", brief(&other)),
        }
    }
    assert_eq!(
        client.diagnostics().unwrap().admitted,
        later,
        "the later replies submitted no owner job: no Revoke, no Close"
    );
    // Nothing was revoked or closed behind the retained unit.
    let (state, closing) = (mount_state(&client, route, mount), cleanup(&client, route));
    let last = h.engine(helper);
    let end = client.diagnostics().unwrap();
    println!(
        "{name}: stage=Requests detached=true retained=1 later_replies_equal=2 engine_mount={state:?} cleanup={closing:?} namespaces={}(baseline {}) closed_namespaces={}->{}",
        last.namespaces, empty.namespaces, start.closed_namespaces, end.closed_namespaces
    );
    assert_eq!(state, NativeMountState::Live);
    assert_eq!(closing, CleanupState::Live);
    assert_eq!(last.namespaces, empty.namespaces + 1);
    assert_eq!(end.closed_namespaces, start.closed_namespaces);
    assert_eq!(h.serving.work().unwrap().mounts, 1, "the lane is kept");
    drop(guard);
    let closed = h.try_unmount(helper).unwrap();
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
    let Harness {
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
    checks.done(&name);
}

/// FP-31, the kernel's FORGET of a reference the engine no longer records:
/// the decrement is refused, the unit is retained, nothing is guessed, and
/// the later Unmount stops `Retained` at `Requests`.
#[test]
fn fp31_a_kernel_forget_of_a_row_the_owner_port_removed_is_retained_and_unmount_stops_at_requests()
{
    refused_at_the_service(Reduced::Absent);
}

/// FP-31, the literal underflow branch: the kernel's one unit of 2 against a
/// row the owner port reduced to 1.
#[test]
fn fp31_a_kernel_forget_of_two_against_a_row_of_one_underflows_and_the_row_keeps_exactly_one() {
    refused_at_the_service(Reduced::TooSmall);
}

// ------------------------------------------------------------ session scope

fn counts(client: &OwnerClient, route: Route) -> StoredCounts {
    let done = job(client, route, Command::Resources { global: true });
    match done.result() {
        Ok(Response::Resources(resources)) => resources.counts,
        other => panic!("{other:?}"),
    }
}
/// The original failure of every retained request of one lane, as text:
/// the request's own description and its source chain.
fn retained(queue: &MountQueue) -> Vec<String> {
    (0..HANDOFFS)
        .filter_map(|slot| {
            queue
                .inspect_retained(slot, |view| match view {
                    FailureView::Request(error) => {
                        let mut text = error.to_string();
                        let mut source = error.source();
                        while let Some(cause) = source {
                            text.push_str(&format!(" <- {cause}"));
                            source = cause.source();
                        }
                        text
                    }
                    FailureView::Panic(_) => "a panic".to_owned(),
                })
                .ok()
        })
        .collect()
}
/// One inode of the session-scope mount, staged for one refusal.
struct Staged {
    reduced: Reduced,
    inode: u64,
    /// The kernel's original input, as the retained request describes it.
    input: String,
    /// The engine's original refusal of that input.
    cause: &'static str,
}

/// The session scope. The same real kernel mount, attached through the
/// public pieces the Service composes, with the lane handle kept so that
/// `MountQueue::inspect_retained` shows each retained unit's original input
/// and original cause; then the session's own detach and drain.
///
/// A retained request fences its lane (`layerfs-fuse` `dispatch/task.rs`
/// sets `terminal`), and a FORGET unit received on a fenced lane is
/// disposed `Unadmitted` with no decrement attempted (`request/inline.rs`:
/// "the kernel reference stays in indexed custody until drain-qualified
/// revocation. No decrement is guessed"). With one staged inode exactly one
/// unit is retained. With two, the second unit is retained as well only if
/// it was admitted before the first was refused, which is the kernel's and
/// the workers' timing: every unit is then either retained with its own
/// input and cause or unadmitted, and neither changes the engine.
fn refused_at_the_session(label: &str, variants: &[Reduced]) {
    let name = format!("FP-31 session {variants:?}");
    let reclaim = Reclaim::own();
    let fixture = installed::Fixture::built(label, |source| {
        for name in ["single", "pair"] {
            fs::write(source.join(name), format!("{name}\n")).unwrap();
            fs::set_permissions(source.join(name), fs::Permissions::from_mode(0o644)).unwrap();
        }
        fs::hard_link(source.join("pair"), source.join("alias")).unwrap();
        3
    });
    let store = open_store(
        fixture.config.clone(),
        installed::BINDING,
        installed::CURSOR,
        2,
        2 * 1024 * 1024,
        ReservationBlocks::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &fixture.directory.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let bound = Arc::new(
        store
            .bind(
                client.clone(),
                BindRequest {
                    branch: fixture.branch,
                    workspace: WorkspaceId::from_authority([7; 32]).unwrap(),
                },
            )
            .unwrap()
            .workspace,
    );
    let route = bound.route();
    let base_root = bound
        .operation()
        .unwrap()
        .workspace()
        .base()
        .unwrap()
        .root()
        .root_inode()
        .serial();
    let created = job(
        &client,
        route,
        Command::Native(NativeJob::Mount { root: base_root }),
    );
    let mount = match created.result() {
        Ok(Response::Native(NativeReply::Mount(mount))) => *mount,
        other => panic!("{other:?}"),
    };
    drop(created);
    let mut pool = Dispatch::start(DispatchConfig {
        read_handles: 2,
        namespaces: 1,
    })
    .unwrap_or_else(|failure| panic!("{failure:?}"));
    let queue = pool.register(mount).unwrap();
    let directory = fixture.directory.join("mount");
    let session = NativeSession::attach(
        SessionConfig {
            directory: directory.clone(),
            owner_uid: Uid::effective().as_raw(),
            owner_gid: Gid::effective().as_raw(),
            ready_wait: Duration::from_secs(3),
            drain_wait: Duration::from_secs(3),
        },
        queue.clone(),
        bound.clone(),
        Identity {
            root: base_root,
            uid: COMMAND,
            gid: COMMAND,
        },
    )
    .unwrap_or_else(|failure| panic!("{failure:?}"));
    let guard = Mounted(directory.to_str().unwrap().to_owned());
    assert!(mount_entry(&guard.0).is_some());
    let lane = || queue.work().unwrap();
    let idle = |what: &str| {
        let mut last = None;
        within(what, || {
            let now = lane();
            let settled = now.received == 0 && now.admitted == 0 && last == Some(now.completed);
            last = Some(now.completed);
            thread::sleep(Duration::from_millis(10));
            settled
        });
    };
    idle("the fresh connection is quiet");
    let baseline = counts(&client, route);

    // One LOOKUP per name, then each count reduced by one through the
    // public owner port: a row of 1 is gone, a row of 2 is kept with 1.
    let staged: Vec<Staged> = variants
        .iter()
        .map(|&reduced| match reduced {
            Reduced::Absent => {
                let inode = fs::symlink_metadata(directory.join("single"))
                    .unwrap()
                    .ino();
                Staged {
                    reduced,
                    inode,
                    input: format!("Forget {{ inode: {inode}, count: 1 }}"),
                    cause: "Stale",
                }
            }
            Reduced::TooSmall => {
                let inode = fs::symlink_metadata(directory.join("pair")).unwrap().ino();
                let alias = fs::symlink_metadata(directory.join("alias")).unwrap();
                assert_eq!(
                    (alias.ino(), alias.nlink()),
                    (inode, 2),
                    "one inode, two names"
                );
                Staged {
                    reduced,
                    inode,
                    input: format!("Forget {{ inode: {inode}, count: 2 }}"),
                    cause: "native lookup underflow",
                }
            }
        })
        .collect();
    let units = staged.len();
    let kept = staged
        .iter()
        .filter(|unit| unit.reduced == Reduced::TooSmall)
        .count() as u64;
    idle("the lookups are answered");
    let looked = counts(&client, route);
    assert_eq!(
        looked.owner_details - baseline.owner_details,
        2 * units as u64,
        "the lookup and custody rows of each kernel inode: {baseline:?} {looked:?}"
    );
    for unit in &staged {
        assert_eq!(
            decrement(&client, route, mount, serial(unit.inode), 1),
            Ok(())
        );
    }
    let stolen = counts(&client, route);
    assert_eq!(stolen.owner_details, baseline.owner_details + 2 * kept);
    let before_forget = lane();
    assert!(retained(&queue).is_empty());

    // The kernel's own FORGET of each inode, delivered and disposed.
    let mut requests = 0;
    before("every FORGET unit was delivered", RECLAIM, || {
        requests += 1;
        reclaim.request();
        let now = lane();
        session.facts().opcodes.forget_units == units as u64
            && now.received == 0
            && now.admitted == now.retained
    });
    let refused = lane();
    let opcodes = session.facts().opcodes;
    let texts = retained(&queue);
    println!(
        "{name}: inodes={:?} reclaim_requests={requests} lane {before_forget:?} -> {refused:?} forget_units={} unadmitted={} handoffs={}",
        staged.iter().map(|unit| unit.inode).collect::<Vec<_>>(),
        opcodes.forget_units,
        opcodes.unadmitted,
        opcodes.handoffs
    );
    for text in &texts {
        println!("{name}: retained unit: {text}");
    }
    // Every unit is retained or, behind the fence of an earlier one,
    // unadmitted; none completed.
    assert_eq!(refused.completed, before_forget.completed, "{refused:?}");
    assert_eq!(refused.admitted, refused.retained);
    assert_eq!(texts.len(), refused.retained);
    assert!(refused.terminal, "a retained request fences its lane");
    assert_eq!(
        refused.retained as u64 + opcodes.unadmitted,
        units as u64,
        "{refused:?} {opcodes:?}"
    );
    if units == 1 {
        assert_eq!((refused.retained, opcodes.unadmitted), (1, 0));
    } else {
        assert!(refused.retained >= 1);
    }
    // The original input of each retained unit, and the engine's original
    // refusal of exactly that input.
    for text in &texts {
        let unit = staged
            .iter()
            .find(|unit| text.contains(&unit.input))
            .unwrap_or_else(|| panic!("{name}: a retained unit with no staged input: {text}"));
        assert!(
            text.contains(unit.cause),
            "{name}: {:?}: not the engine's refusal {:?}: {text}",
            unit.reduced,
            unit.cause
        );
    }
    // No failed and no unadmitted unit changed the engine.
    assert_eq!(counts(&client, route), stolen);
    assert_eq!(mount_state(&client, route, mount), NativeMountState::Live);

    // The session's own detach and drain.
    let mut session = session;
    assert_eq!(session.detach(), Ok(Detach::Detached));
    assert!(mount_entry(&guard.0).is_none());
    let undrained = match session.drain() {
        Err(undrained) => *undrained,
        Ok(drained) => panic!("{name}: drained with a retained unit: {drained:?}"),
    };
    println!("{name}: first drain: {}", brief(&undrained));
    let Undrained {
        session,
        stage,
        work,
        failed_demands,
        ..
    } = undrained;
    assert_eq!(stage, DrainStage::Requests);
    let stopped = work.expect("lane counters at the stop");
    assert_eq!(
        (stopped.received, stopped.admitted, stopped.retained),
        (0, refused.retained, refused.retained)
    );
    assert_eq!(failed_demands.count, 0, "not a failed base demand");
    assert_eq!(retained(&queue), texts, "the same custody after the stop");
    // Repeated, the drain observes the same predicate and stops again.
    let again = match session.drain() {
        Err(undrained) => *undrained,
        Ok(drained) => panic!("{name}: the repeated drain released the lane: {drained:?}"),
    };
    assert_eq!(again.stage, DrainStage::Requests);
    assert_eq!(
        again
            .work
            .map(|work| (work.received, work.admitted, work.retained)),
        Some((0, refused.retained, refused.retained))
    );
    assert_eq!(retained(&queue), texts, "the same custody after the repeat");
    // Still not revoked, and nothing was guessed: each row is exactly as the
    // owner port left it. A kept row returns its one count and then has
    // none; a removed row has none.
    assert_eq!(mount_state(&client, route, mount), NativeMountState::Live);
    assert_eq!(cleanup(&client, route), CleanupState::Live);
    assert_eq!(counts(&client, route), stolen);
    for unit in &staged {
        if unit.reduced == Reduced::TooSmall {
            assert_eq!(
                decrement(&client, route, mount, serial(unit.inode), 1),
                Ok(())
            );
        }
        let none = decrement(&client, route, mount, serial(unit.inode), 1);
        assert!(
            none.as_ref().is_err_and(|error| error.contains("Stale")),
            "{name}: {:?}: {none:?}",
            unit.reduced
        );
    }
    assert_eq!(counts(&client, route).owner_details, baseline.owner_details);
    println!(
        "{name}: stage=Requests twice retained={} unadmitted={} engine counts unchanged by every unit; rows exactly as reduced; engine_mount=Live cleanup=Live",
        refused.retained, opcodes.unadmitted
    );

    // The retained lane keeps the dispatcher from a clean stop; the owners
    // are dropped with that custody, as a retained Service entry is.
    assert!(matches!(queue.finish(), Err(DispatchError::Busy)));
    assert!(matches!(pool.stop(), Err(DispatchError::Busy)));
    drop(again);
    drop(guard);
    drop(queue);
    drop(pool);
    drop(bound);
    owner.stop().unwrap();
    drop(store);
    fixture.cleanup();
}

/// FP-31, the retained unit's original input: the kernel's unit of 1 for an
/// inode whose row the owner port removed is kept as `Forget { inode,
/// count: 1 }` with the engine's `Stale`.
#[test]
fn fp31_the_retained_unit_of_a_removed_row_keeps_the_kernels_input_and_the_engines_stale() {
    refused_at_the_session("fp31-in-absent", &[Reduced::Absent]);
}

/// FP-31, the retained unit's original input at the literal underflow
/// branch: the kernel's unit of 2 is kept as `Forget { inode, count: 2 }`
/// with the engine's "native lookup underflow".
#[test]
fn fp31_the_retained_unit_of_two_keeps_the_kernels_input_and_the_engines_underflow() {
    refused_at_the_session("fp31-in-small", &[Reduced::TooSmall]);
}

/// FP-31, two refused inodes on one mount: each unit is retained with its
/// own input and cause or, received behind the fence of the other, is
/// unadmitted; the engine is changed by neither.
#[test]
fn fp31_two_refused_forget_units_are_each_retained_or_unadmitted_and_change_nothing() {
    refused_at_the_session("fp31-in-both", &[Reduced::Absent, Reduced::TooSmall]);
}
