//! A new Mount is refused `Capacity` / `mount:debt` while automatic
//! maintenance is stopped (specification 4.2, D-13; R8b track P-B).
//!
//! The owner keeps the first failure of an automatic maintenance turn and
//! attempts no further turn. The stage that produces one here uses real
//! resources only, in an order every step of which is waited for:
//!
//! 1. Two namespaces of the test's own are opened through the public owner
//!    client in the same engine. The first publishes once and leaves that
//!    reply unattempted, so a capture of it parks: one admitted job stays
//!    queued. While a job is queued, every later job is served by the owner
//!    thread, and the owner thread's next turn with no runnable job is a
//!    maintenance turn.
//! 2. The engine's backing file is renamed aside and another file is written
//!    at its path, as `resources_observation.rs` does: a real external change
//!    of the path identity the engine checks before every write.
//! 3. One publication on the second namespace is that write (a mutation of
//!    the first would wait behind its capture). Its admission reads the
//!    changed identity, so the job fails with unknown outcome and the engine
//!    quarantines itself.
//! 4. The owner thread's next maintenance turn finds the engine unavailable.
//!    That is the retained first failure: maintenance is stopped. The capture
//!    stays parked: its reply is still unattempted.
//!
//! No product hook, fault injection, mocked engine or replay is involved,
//! and no kernel mount is needed: the refusal is decided at control
//! admission. What is not staged: a maintenance turn that is itself the
//! first to meet the failing resource. A turn is taken as soon as the job
//! that made it pending has committed, on another thread, so a change placed
//! between the two would be a race.
//!
//! Every control call below travels one real authenticated channel, so the
//! refusal is asserted as the bytes a controller receives, beside the
//! daemon's own receipt of the same call.
#[allow(dead_code)]
#[path = "support/control_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use layerfs_bridge::control::{Activity, ControlCode, ControlRefusal, Reply, Request};
use layerfs_daemon::{
    control::{Failure, Service},
    Command, Completion, OwnerClient, OwnerError, OwnerWork, Pending, Response,
};
use layerfs_history::{BranchId, WorkspaceId};
use layerfs_overlay::{Inode, InodeKind, OverlayError, Publication, Route};
use layerfs_sdk::control::Control;
use std::{
    fs,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(10);

fn identity(tag: u8) -> WorkspaceId {
    WorkspaceId::from_authority([tag; 32]).unwrap()
}
fn mount(tag: u8, branch: BranchId) -> Request {
    Request::Mount {
        workspace: identity(tag),
        branch,
    }
}
/// The daemon's own receipt of one served call, read on the serving thread.
#[derive(Debug, Eq, PartialEq)]
enum Receipt {
    Success,
    /// A typed refusal with its phase, the engine completions the operation
    /// attempted and whether the original cause is kept with it.
    Refused {
        code: ControlCode,
        phase: &'static str,
        detail: String,
        completions: usize,
        original: bool,
    },
    Other(String),
}
/// One control call over a real authenticated channel, served once by the
/// product's own control route: the reply as received, and the receipt. The
/// serving thread ends with its one call or with its socket's own timeout.
fn call(service: &Arc<Service>, request: Request) -> (Reply, Receipt) {
    let serving = service.clone();
    let (channel, worker) = support::pair(move |connection| {
        let served = serving.serve_one_control(connection).unwrap();
        match &served.outcome {
            Ok(_) => Receipt::Success,
            Err(Failure::Native(refused)) => Receipt::Refused {
                code: refused.code,
                phase: refused.phase,
                detail: refused.detail.clone(),
                completions: refused.completions.len(),
                original: refused.evidence.is_some(),
            },
            Err(other) => Receipt::Other(format!("{other:?}")),
        }
    });
    let reply = Control::new(channel).call(request).unwrap();
    (reply, worker.join())
}
fn submit(client: &OwnerClient, route: Option<Route>, command: Command) -> Pending {
    match client.try_submit(route, command) {
        Ok(pending) => pending,
        Err((error, _)) => panic!("admission: {error:?}"),
    }
}
/// A completion, observed with a bound: no wait depends on another thread.
fn done(pending: &Pending, what: &str) -> Completion {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < deadline, "{what}: no completion");
        thread::sleep(Duration::from_millis(1));
    }
}
fn publish(client: &OwnerClient, route: Route, serial: u64) -> Completion {
    let inode = Inode {
        serial,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: 1,
        mtime_nanoseconds: 2,
        nlink: 1,
        size: 0,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
        subdirs: 0,
    };
    let pending = submit(
        client,
        Some(route),
        Command::Publish {
            inode,
            name: None,
            cell: None,
        },
    );
    done(&pending, "publish")
}
/// The owner counters a refused Mount must leave as they were.
fn admission(work: &OwnerWork) -> (u64, [u64; 6], usize, usize, u64) {
    (
        work.admitted,
        work.completed,
        work.outstanding,
        work.queued,
        work.maintenance_jobs,
    )
}

#[test]
fn a_new_mount_is_refused_capacity_at_mount_debt_while_maintenance_is_stopped() {
    let f = fixture::Fixture::new("mount-debt");
    let branch = f.native.project.branch.branch.id;
    let client = f.owner.client();
    let store = f.installed.opened.store.clone();

    // Control: before any failure, Mounts of this shape are admitted. Each
    // is one owner job and at least one Store statement.
    let (anchor, anchor_binding) = match call(&f.service, mount(71, branch)) {
        (Reply::Bound { token, binding }, Receipt::Success) => (token, binding),
        other => panic!("anchor Mount: {other:?}"),
    };
    let before = (client.diagnostics().unwrap(), f.statements());
    let admitted = call(&f.service, mount(72, branch));
    let after = (client.diagnostics().unwrap(), f.statements());
    assert!(
        matches!(admitted, (Reply::Bound { .. }, Receipt::Success)),
        "{admitted:?}"
    );
    assert_eq!(after.0.admitted - before.0.admitted, 1, "one Open job");
    assert!(after.1 > before.1, "a Mount reads History");
    assert!(client.maintenance_failure().unwrap().is_none());
    println!(
        "MOUNT_DEBT control: Mount admitted, owner admitted {} -> {}, Store statements {} -> {}, maintenance_failure=None",
        before.0.admitted, after.0.admitted, before.1, after.1
    );

    // 1. The test's own namespaces: in one, a publication whose reply is
    //    not attempted and a capture that parks behind it.
    let open = |tag: u8| {
        let opened = done(
            &submit(
                &client,
                None,
                Command::Open {
                    incarnation: [tag; 32],
                    base_root: [17; 32],
                },
            ),
            "open",
        );
        match opened.result() {
            Ok(Response::Opened(route)) => *route,
            other => panic!("open: {other:?}"),
        }
    };
    let (route, other) = (open(201), open(202));
    let first = publish(&client, route, 2);
    let unattempted: Publication = match first.result() {
        Ok(Response::Published(publication)) => *publication,
        other => panic!("publish: {other:?}"),
    };
    drop(first);
    let attempts = client
        .diagnostics()
        .unwrap()
        .sql_foreground
        .total()
        .attempts;
    let capture = submit(&client, Some(route), Command::Capture);
    let deadline = Instant::now() + WAIT;
    loop {
        let work = client.diagnostics().unwrap();
        if work.queued == 1 && work.sql_foreground.total().attempts > attempts {
            break;
        }
        assert!(Instant::now() < deadline, "the capture never parked");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(capture.try_complete().unwrap().is_none());

    // 2. A real external change of the backing file's path identity.
    let backing = f.native.directory.join("overlay.sqlite");
    fs::rename(&backing, f.native.directory.join("original-overlay.sqlite")).unwrap();
    fs::write(&backing, b"different physical inode").unwrap();
    assert!(client.maintenance_failure().unwrap().is_none());

    // 3. The next write meets it at its own admission.
    let failed = publish(&client, other, 2);
    let write_failure = format!("{:?}", failed.result().as_ref().err());
    assert!(
        matches!(
            failed.result(),
            Err(OwnerError::Overlay(OverlayError::Uncertain { .. }))
        ) && write_failure.contains("allocation path identity"),
        "{write_failure}"
    );
    drop(failed);

    // 4. The owner thread's next maintenance turn: observed, bounded.
    let deadline = Instant::now() + WAIT;
    let stopped = loop {
        if let Some(stopped) = client.maintenance_failure().unwrap() {
            break stopped;
        }
        assert!(
            Instant::now() < deadline,
            "no maintenance failure was retained: {:?}",
            admission(&client.diagnostics().unwrap())
        );
        thread::sleep(Duration::from_millis(1));
    };
    assert!(matches!(*stopped, OverlayError::Quarantined), "{stopped:?}");
    // Only the parked capture is left: queued, and held by this test.
    let deadline = Instant::now() + WAIT;
    let quiet = loop {
        let work = client.diagnostics().unwrap();
        if work.outstanding == 1 && work.queued == 1 {
            break work;
        }
        assert!(Instant::now() < deadline, "{:?}", admission(&work));
        thread::sleep(Duration::from_millis(1));
    };
    assert!(capture.try_complete().unwrap().is_none());
    println!(
        "MOUNT_DEBT stage: unattempted publication revision={} write under the changed identity={write_failure} maintenance_failure={stopped:?} owner (admitted, completed, outstanding, queued, maintenance_jobs)={:?}",
        unattempted.revision(),
        admission(&quiet)
    );

    // The refusal: typed, at its phase, before any effect.
    let expected = ControlRefusal {
        code: ControlCode::Capacity,
        phase: "mount:debt".into(),
        moved: None,
        published: None,
        detail: stopped.to_string(),
    };
    for tag in [73, 74, 73] {
        let before = (
            admission(&client.diagnostics().unwrap()),
            f.statements(),
            store.work(),
        );
        let (reply, receipt) = call(&f.service, mount(tag, branch));
        let after = (
            admission(&client.diagnostics().unwrap()),
            f.statements(),
            store.work(),
        );
        println!(
            "MOUNT_DEBT Mount {tag}: reply={reply:?} receipt={receipt:?} owner (admitted, completed, outstanding, queued, maintenance_jobs) {:?} -> {:?} Store statements {} -> {}",
            before.0, after.0, before.1, after.1
        );
        assert_eq!(reply, Reply::Refused(expected.clone()));
        assert_eq!(
            receipt,
            Receipt::Refused {
                code: ControlCode::Capacity,
                phase: "mount:debt",
                detail: stopped.to_string(),
                completions: 0,
                original: true,
            }
        );
        assert_eq!(after.0, before.0, "no owner job was submitted");
        assert_eq!(after.1, before.1, "no Store statement was issued");
        assert_eq!(after.2, before.2, "no Store demand was made");
        // No registry entry: the incarnation is absent, not retained.
        match call(&f.service, Request::Locate(identity(tag))) {
            (Reply::Refused(absent), _) => assert_eq!(
                (absent.code, absent.detail.as_str()),
                (ControlCode::Missing, "Workspace absent")
            ),
            other => panic!("Locate {tag}: {other:?}"),
        }
    }
    // An incarnation that is already admitted keeps its own answer.
    match call(&f.service, mount(71, branch)) {
        (Reply::Refused(duplicate), _) => assert_eq!(
            (duplicate.code, duplicate.phase.as_str()),
            (ControlCode::Busy, "admission")
        ),
        other => panic!("duplicate Mount: {other:?}"),
    }

    // The Workspace bound before the failure still answers Status, with its
    // control fields; the engine's own observation is its original refusal.
    let status = match call(&f.service, Request::Status(anchor)) {
        (Reply::Status(status), Receipt::Success) => *status,
        other => panic!("Status: {other:?}"),
    };
    println!(
        "MOUNT_DEBT anchor Status: activity={:?} epoch={} local={:?} local_failure={:?}",
        status.activity, status.epoch, status.local, status.local_failure
    );
    assert_eq!(status.token, anchor);
    assert_eq!(status.binding, anchor_binding);
    assert_eq!(status.activity, Activity::Idle);
    assert!(status.local.is_none() && status.local_failure.is_some());
    assert!(matches!(
        *client.maintenance_failure().unwrap().unwrap(),
        OverlayError::Quarantined
    ));

    // The engine is quarantined: the owner is stopped as process scope, with
    // a bound, and whatever it answers is recorded, not required.
    let fixture::Fixture {
        native,
        installed,
        owner,
        service,
    } = f;
    drop((service, client, store, stopped));
    let (send, joined) = mpsc::channel();
    thread::spawn(move || {
        let _ = send.send(format!("{:?}", owner.stop()));
    });
    match joined.recv_timeout(WAIT) {
        Ok(answer) => println!(
            "MOUNT_DEBT owner stop: {answer}; parked capture answered={:?}",
            capture.try_complete().map(|done| done.is_some())
        ),
        Err(_) => panic!("the owner did not stop"),
    }
    drop(capture);
    drop(installed);
    native.cleanup();
}
