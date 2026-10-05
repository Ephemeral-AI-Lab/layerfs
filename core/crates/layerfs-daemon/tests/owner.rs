//! Deterministic public owner/service proofs, independent of unbuilt FUSE/control.
use layerfs_daemon::{
    Command, Completion, Owner, OwnerClient, OwnerConfig, OwnerError, Pending, Response,
};
use layerfs_overlay::{Inode, InodeKind, ProfileConfig, Publication, Route};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-owner-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn submit(client: &OwnerClient, route: Option<Route>, command: Command) -> Pending {
    match client.try_submit(route, command) {
        Ok(p) => p,
        Err((error, _)) => panic!("admission: {error:?}"),
    }
}
fn open(client: &OwnerClient, tag: u8) -> Route {
    let done = submit(
        client,
        None,
        Command::Open {
            incarnation: [tag; 32],
            base_root: [17; 32],
        },
    )
    .wait()
    .unwrap();
    match done.result() {
        Ok(Response::Opened(route)) => *route,
        x => panic!("open: {x:?}"),
    }
}
fn value(serial: u64) -> Inode {
    Inode {
        serial,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: 1,
        mtime_nanoseconds: 2,
        nlink: 1,
        size: 0,
        inherited_cutoff: 0,
    }
}
fn write(client: &OwnerClient, route: Route, serial: u64) -> Completion {
    submit(
        client,
        Some(route),
        Command::Publish {
            inode: value(serial),
            name: None,
            cell: None,
        },
    )
    .wait()
    .unwrap()
}
fn publication(done: &Completion) -> Publication {
    match done.result() {
        Ok(Response::Published(p)) => *p,
        x => panic!("publication: {x:?}"),
    }
}
fn reply(client: &OwnerClient, p: Publication) {
    let done = submit(client, Some(p.route()), Command::ReplyAttempted(p))
        .wait()
        .unwrap();
    assert!(matches!(done.result(), Ok(Response::Done)));
}

#[test]
fn parked_capture_allows_unrelated_progress_and_includes_earlier_queued_mutation() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("overlay"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    assert_eq!(owner.profile().schema_version, 3);
    let client = owner.client();
    let a = open(&client, 1);
    let b = open(&client, 2);
    let mutation = submit(
        &client,
        Some(a),
        Command::Publish {
            inode: value(2),
            name: None,
            cell: None,
        },
    );
    let capture = submit(&client, Some(a), Command::Capture);
    let later = submit(
        &client,
        Some(a),
        Command::Publish {
            inode: value(3),
            name: None,
            cell: None,
        },
    );
    let published = mutation.wait().unwrap();
    let p = publication(&published);
    // Capture has no published/reply-complete frontier yet. It is parked rather
    // than attempted/refused/replayed, and another Workspace remains runnable.
    assert!(capture.try_complete().unwrap().is_none());
    assert!(later.try_complete().unwrap().is_none());
    let other = submit(&client, Some(b), Command::State).wait().unwrap();
    assert!(matches!(other.result(), Ok(Response::State(_))));
    reply(&client, p);
    let captured = capture.wait().unwrap();
    let c = match captured.result() {
        Ok(Response::Captured(c)) => *c,
        x => panic!("capture: {x:?}"),
    };
    assert_eq!(c.revision, p.revision());
    let later = later.wait().unwrap();
    let later_publication = publication(&later);
    assert!(later_publication.generation.number() > c.generation.number());
    reply(&client, later_publication);
    let page = submit(
        &client,
        Some(a),
        Command::CapturedInodes {
            capture: c,
            after: 0,
        },
    )
    .wait()
    .unwrap();
    assert!(matches!(page.result(),Ok(Response::Inodes(rows)) if rows==&vec![value(2)]));
    let counters = client.diagnostics().unwrap();
    assert!(counters.completed.iter().sum::<u64>() >= 7);
    owner.stop().unwrap();
}

#[test]
fn result_credits_and_reserved_lifecycle_capacity_survive_ordinary_saturation() {
    let temp = Temp::new();
    let config = OwnerConfig {
        jobs_per_namespace: 2,
        ..OwnerConfig::default()
    };
    let owner = Owner::start(&temp.0.join("overlay"), ProfileConfig::default(), config).unwrap();
    let client = owner.client();
    let a = open(&client, 3);
    let b = open(&client, 4);
    let first = write(&client, a, 2);
    let second = write(&client, a, 3);
    let p1 = publication(&first);
    let p2 = publication(&second);
    assert_eq!(client.diagnostics().unwrap().outstanding, 2);
    let rejected = client.try_submit(Some(a), Command::Inode(2));
    assert!(matches!(
        rejected,
        Err((OwnerError::AdmissionFull, Command::Inode(2)))
    ));
    // Caller-held completed ordinary results still own credits, but release/
    // reply work has separate slots/bytes and can settle original publication.
    reply(&client, p1);
    reply(&client, p2);
    let other = submit(&client, Some(b), Command::State).wait().unwrap();
    assert!(matches!(other.result(), Ok(Response::State(_))));
    drop(other);
    drop(first);
    drop(second);
    assert_eq!(client.diagnostics().unwrap().outstanding, 0);
    assert!(client.diagnostics().unwrap().peak_credited_bytes <= config.bytes);
    let read = submit(&client, Some(a), Command::Inode(2)).wait().unwrap();
    assert!(matches!(read.result(), Ok(Response::Inode(Some(_)))));
    owner.stop().unwrap();
}

#[test]
fn stopping_cancels_unattempted_capture_and_fences_future_admission() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("overlay"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let a = open(&client, 5);
    let published = write(&client, a, 2);
    let capture = submit(&client, Some(a), Command::Capture);
    assert!(capture.try_complete().unwrap().is_none());
    owner.stop().unwrap();
    let result = capture.wait().unwrap();
    assert!(matches!(result.result(), Err(OwnerError::Stopped)));
    assert!(matches!(
        client.try_submit(Some(a), Command::State),
        Err((OwnerError::Stopped, _))
    ));
    assert!(matches!(published.result(), Ok(Response::Published(_))));
}
