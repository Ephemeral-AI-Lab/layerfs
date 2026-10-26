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
//! count is reduced by `|job_client: &layerfs_daemon::OwnerClient, job_route| job_client.kernel_forget(job_route)` through the owner client, and the
//! kernel's FORGET is then asked for by reclaim of the test container's own
//! memory cgroup (`memory.reclaim`; never a system-wide cache drop).
//!
//! - **Row absent.** One lookup, count 1, reduced by 1: the row is gone, and
//!   the kernel's unit of 1 finds none (`OverlayError::Stale`).
//! - **Row too small, the literal underflow branch.** One inode with two
//!   names, each looked up once: count 2, reduced by 1. The row is present
//!   with count 1, and the kernel's one unit of 2 underflows it
//!   (`layerfs-overlay` `lifetime/kernel.rs`, `checked_sub`).
//!
//! Two scopes, because of what each can observe:
//!
//! - The first two tests use the control Service, as every mounted proof
//!   does: the unit is retained, the engine's counts are unchanged by the
//!   failed job, no decrement is guessed, and the later Unmount stops
//!   `Retained` at `Requests` (`layerfs-fuse` `session/drain.rs`) and repeats
//!   that custody. The daemon exposes no observation of a retained request's
//!   original failure (`MountRequests::inspect_retained` has no caller in daemon
//!   source), so the unit's original input cannot be read at this scope.
//! - The three session tests attach the same real kernel mount through the
//!   public pieces the Service composes (`Dispatch`, `KernelSession::attach`
//!   over the bound Workspace's own request services), keep the lane handle
//!   and read each retained unit's original input and original cause through
//!   `MountRequests::inspect_retained`. Each then makes the session's own detach
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
//! Caveat, stated as the audit stated it: this proves custody of a kernel
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
    Activity, MountPhase, MountWork, Reply, Request, TeardownStage, WorkspaceToken,
};
use layerfs_daemon::{control::Failure, OwnerClient};
use layerfs_overlay::{CleanupState, KernelMount, KernelMountState};
use mounted::{mount_entry, Harness};
use rig::{root, stamp_tree, Rig};
use std::{
    fmt::Debug,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command as Process,
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
fn job<T>(
    client: &OwnerClient,
    route: Route,
    command: impl FnOnce(
        &layerfs_daemon::OwnerClient,
        layerfs_workspace::Route,
    ) -> Result<T, layerfs_daemon::OwnerError>,
) -> T {
    command(client, route).unwrap_or_else(|error| panic!("{error:?}"))
}
fn engine_mount(client: &OwnerClient, route: Route) -> Option<KernelMount> {
    job(
        client,
        route,
        |job_client: &layerfs_daemon::OwnerClient, job_route| {
            job_client.kernel_retained_mount(job_route)
        },
    )
}
fn mount_state(client: &OwnerClient, route: Route, mount: KernelMount) -> KernelMountState {
    job(
        client,
        route,
        |job_client: &layerfs_daemon::OwnerClient, job_route| {
            job_client.kernel_state(job_route, mount)
        },
    )
}
fn cleanup(client: &OwnerClient, route: Route) -> CleanupState {
    job(
        client,
        route,
        |job_client: &layerfs_daemon::OwnerClient, job_route| job_client.cleanup_state(job_route),
    )
}
/// One lookup decrement through the public owner port, as
/// `kernel_jobs.rs` submits it: the engine's original answer as text, `Ok`
/// when the job succeeded.
fn decrement(
    client: &OwnerClient,
    route: Route,
    mount: KernelMount,
    serial: u64,
    count: u64,
) -> Result<(), String> {
    client
        .kernel_forget(route, mount, serial, count)
        .map_err(|error| format!("{error:?}"))
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

/// The engine's stored counts beside the kernel lookup entries and file
/// custody it keeps in memory, observed through a live route.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Seen {
    counts: layerfs_overlay::StoredCounts,
    owner_details: u64,
}
fn seen(client: &OwnerClient) -> Seen {
    let resources = client.resources().unwrap();
    Seen {
        counts: resources.counts,
        owner_details: resources.open.kernel_entries + resources.open.file_custody,
    }
}
fn work(harness: &Harness, token: WorkspaceToken) -> MountWork {
    harness.status(token).kernel.unwrap().work.unwrap()
}
/// Two consecutive observations with nothing received or admitted and no
/// request completed in between. A readiness wait before one attempt.
fn quiet(harness: &Harness, token: WorkspaceToken) {
    let mut last = None;
    within("connection quiescent", || {
        let now = work(harness, token);
        let settled = now.active == 0 && last == Some(now.completed);
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
    let empty = h.engine(helper);
    let start = client.diagnostics().unwrap();
    let ready = rig.mount(2);
    let guard = Mounted(ready.directory.clone());
    let token = ready.token;
    let observed = h.service.operation(token).unwrap();
    let route = observed.workspace().route();
    let mut checks = Checks::default();
    quiet(h, token);
    let baseline = seen(observed.overlay());
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
    let looked = seen(observed.overlay());
    // The inode's kernel lookup entry and its file custody.
    assert_eq!(
        looked.owner_details - baseline.owner_details,
        2,
        "{baseline:?} {looked:?}"
    );
    let mount = engine_mount(&client, route).expect("attached engine mount");

    // The engine's count is reduced by one through the public owner port.
    assert_eq!(decrement(&client, route, mount, serial(inode), 1), Ok(()));
    let stolen = seen(observed.overlay());
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
    let jobs = client.diagnostics().unwrap().jobs.iter().sum::<u64>();
    println!(
        "{name} staged: inode={inode} serial={} kernel_count={held} engine_reduced_by=1 owner_details(baseline={} looked={} reduced={}) engine_mount={mount:?}",
        serial(inode),
        baseline.owner_details,
        looked.owner_details,
        stolen.owner_details
    );

    // The kernel's own FORGET of the inode, delivered and disposed. Every
    // Status and every count observation of the test is itself one owner
    // job (`control/status.rs`, `|job_client: &layerfs_daemon::OwnerClient, job_route| job_client.state(job_route)`; `|job_client: &layerfs_daemon::OwnerClient, job_route| job_client.resources(job_route)`), so
    // they are counted and subtracted.
    let (mut requests, mut observations) = (0, 0);
    before("the kernel's FORGET unit was delivered", RECLAIM, || {
        requests += 1;
        observations += 1;
        reclaim.request();
        let now = work(h, token);
        now.forget_units == 1 && (now.active, now.retained) == (0, 1)
    });
    let refused = work(h, token);
    let after = seen(observed.overlay());
    // One Status call and one resource observation of this Workspace.
    observations += 2;
    let submitted = client.diagnostics().unwrap().jobs.iter().sum::<u64>() - jobs - observations;
    println!(
        "{name}: reclaim_requests={requests} kernel work {staged:?} -> {refused:?} owner_jobs_since_staged={submitted} (beside the test's {observations} observations) engine_counts_equal_reduced={}",
        after == stolen
    );
    // Refused and retained: the unit is kept with its failure, uncompleted,
    // and its receive worker is free.
    checks.that(
        (refused.forget_units, refused.retained, refused.active) == (1, 1, 0)
            && refused.terminal == 0
            && refused.completed == staged.completed
            && refused.served == staged.served + 1,
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
            let counts = seen(observed.overlay());
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
        status.kernel.as_ref().map(|kernel| kernel.phase)
    );

    // The later Unmount: detach, then the drain finds the retained unit.
    let later = client.diagnostics().unwrap().jobs.iter().sum::<u64>();
    let custody = match h.try_unmount(token) {
        Err(Failure::Retained(custody)) => *custody,
        Ok(done) => panic!(
            "{name}: unmounted with a retained FORGET unit: {:?}",
            done.reply
        ),
        Err(other) => panic!("{name}: {}", brief(&other)),
    };
    let unmounted = client.diagnostics().unwrap().jobs.iter().sum::<u64>();
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
        (
            layerfs_fuse::RECEIVE_SLOTS as u16,
            layerfs_fuse::RECEIVE_SLOTS as u16
        ),
        "{stopped:?}"
    );
    assert_eq!(
        (stopped.active, stopped.retained, stopped.forget_units),
        (0, 1, 1),
        "{stopped:?}"
    );
    assert!(mount_entry(&ready.directory).is_none());
    let status = h.status(token);
    assert_eq!(status.activity, Activity::Closing);
    let kernel = status.kernel.unwrap();
    assert_eq!(kernel.phase, MountPhase::Retained);
    assert!(kernel.detached);
    assert_eq!(kernel.ready.as_ref(), Some(&ready));
    let kept = h
        .service
        .retained_kernel(token)
        .unwrap()
        .expect("retained owner");
    assert!(kept.contains("stage: Requests"), "{kept}");
    // Later operations answer with the same custody and attempt nothing.
    let later = client.diagnostics().unwrap().jobs.iter().sum::<u64>();
    for request in [
        Request::Unmount(layerfs_bridge::control::UnmountControl {
            token,
            discard: true,
        }),
        Request::Attach(token),
    ] {
        match h.service.execute_control(&request) {
            Err(Failure::Retained(again)) => assert_eq!(*again, custody, "{request:?}"),
            other => panic!("{name}: {request:?}: {}", brief(&other)),
        }
    }
    assert_eq!(
        client.diagnostics().unwrap().jobs.iter().sum::<u64>(),
        later,
        "the later replies submitted no owner job: no Revoke, no Close"
    );
    // Nothing was revoked or closed behind the retained unit.
    let (state, closing) = (mount_state(&client, route, mount), cleanup(&client, route));
    let last = observed.overlay().resources().unwrap().counts;
    let end = client.diagnostics().unwrap();
    println!(
        "{name}: stage=Requests detached=true retained=1 later_replies_equal=2 engine_mount={state:?} cleanup={closing:?} namespaces={}(baseline {}) closed_namespaces={}->{}",
        last.namespaces, empty.namespaces, start.closed_namespaces, end.closed_namespaces
    );
    assert_eq!(state, KernelMountState::Live);
    assert_eq!(closing, CleanupState::Live);
    assert_eq!(last.namespaces, empty.namespaces);
    assert_eq!(end.closed_namespaces, start.closed_namespaces);
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

use layerfs_workspace::Route;
