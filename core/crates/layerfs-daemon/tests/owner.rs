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

#[test]
fn last_release_starts_automatic_cleanup_during_idle_and_unrelated_live_work() {
    use layerfs_overlay::{CleanupState, Lease, LeaseKind};
    use std::time::{Duration, Instant};
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let target = open(&client, 110);
    let other = open(&client, 111);
    let lease = Lease {
        kind: LeaseKind::Reader,
        owner: 7,
        resource: 0,
    };
    assert!(submit(&client, Some(target), Command::Acquire(lease))
        .wait()
        .unwrap()
        .result()
        .is_ok());
    for serial in 1..=512 {
        let done = write(&client, target, serial);
        let p = match done.result() {
            Ok(Response::Published(p)) => *p,
            x => panic!("{x:?}"),
        };
        drop(done);
        assert!(submit(&client, Some(target), Command::ReplyAttempted(p))
            .wait()
            .unwrap()
            .result()
            .is_ok());
    }
    assert!(submit(&client, Some(target), Command::Close)
        .wait()
        .unwrap()
        .result()
        .is_ok());
    assert!(matches!(
        submit(&client, Some(target), Command::CleanupState)
            .wait()
            .unwrap()
            .result(),
        Ok(Response::CleanupState(CleanupState::Held))
    ));
    assert!(submit(&client, Some(target), Command::Release(lease))
        .wait()
        .unwrap()
        .result()
        .is_ok());
    // Verification stop fence only. Status is observation; no explicit reclaim command exists.
    let stop = Instant::now() + Duration::from_secs(2);
    let mut progress = 0;
    loop {
        assert!(Instant::now() < stop, "automatic cleanup did not progress");
        let state = submit(&client, Some(target), Command::CleanupState)
            .wait()
            .unwrap();
        if matches!(
            state.result(),
            Ok(Response::CleanupState(CleanupState::Gone))
        ) {
            break;
        }
        drop(state);
        assert!(submit(&client, Some(other), Command::State)
            .wait()
            .unwrap()
            .result()
            .is_ok());
        progress += 1;
    }
    assert!(client.diagnostics().unwrap().maintenance_rows >= 512);
    assert!(client.maintenance_failure().unwrap().is_none());
    println!(
        "AUTOMATIC_CLOSE unrelated_progress={progress} work={:?}",
        client.diagnostics().unwrap()
    );
    owner.stop().unwrap();
}

#[test]
fn idle_cleanup_runs_without_an_explicit_reclaim_or_status_job() {
    use std::time::{Duration, Instant};
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let route = open(&client, 112);
    for serial in 1..=128 {
        let done = write(&client, route, serial);
        let publication = match done.result() {
            Ok(Response::Published(p)) => *p,
            x => panic!("{x:?}"),
        };
        drop(done);
        assert!(
            submit(&client, Some(route), Command::ReplyAttempted(publication))
                .wait()
                .unwrap()
                .result()
                .is_ok()
        );
    }
    assert!(submit(&client, Some(route), Command::Close)
        .wait()
        .unwrap()
        .result()
        .is_ok());
    let stop = Instant::now() + Duration::from_secs(2);
    // Fixed memory diagnostics do not admit a database/service job or wake it.
    while client.diagnostics().unwrap().maintenance_rows < 130 {
        assert!(Instant::now() < stop, "idle maintenance did not finish");
        std::thread::yield_now();
    }
    assert!(matches!(
        submit(&client, Some(route), Command::CleanupState)
            .wait()
            .unwrap()
            .result(),
        Ok(Response::CleanupState(layerfs_overlay::CleanupState::Gone))
    ));
    assert!(client.maintenance_failure().unwrap().is_none());
    owner.stop().unwrap();
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
        born: 0,
        entries: 0,
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
    assert_eq!(owner.profile().schema_version, 8);
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
    assert!(
        matches!(result.result(), Err(OwnerError::Unattempted {cause,command}) if matches!(cause.as_ref(),OwnerError::Stopped)&&matches!(command.as_ref(),Command::Capture))
    );
    assert!(matches!(
        client.try_submit(Some(a), Command::State),
        Err((OwnerError::Stopped, _))
    ));
    assert!(matches!(published.result(), Ok(Response::Published(_))));
}

#[test]
fn lost_internal_completions_keep_exact_backed_custody_and_release_result_credits() {
    use layerfs_overlay::{Cell, Dentry, OverlayError, CELL_BYTES, MASK_BYTES};
    use std::time::{Duration, Instant};
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let route = open(&client, 120);
    let other = open(&client, 121);
    // Receiver disappears before the worker's result send. This does not cancel
    // an admitted attempted operation or release its separate DB reply ticket.
    drop(submit(
        &client,
        Some(route),
        Command::Publish {
            inode: value(2),
            name: Some(Dentry {
                parent: 1,
                name: vec![0xff; 255],
                serial: Some(2),
            }),
            cell: Some(Cell {
                offset: 0,
                data: Box::new([17; CELL_BYTES]),
                validity: Box::new([255; MASK_BYTES]),
            }),
        },
    ));
    let stop = Instant::now() + Duration::from_secs(2);
    while client.diagnostics().unwrap().outstanding != 0 {
        assert!(
            Instant::now() < stop,
            "lost completion kept resident credits"
        );
        std::thread::yield_now();
    }
    let tickets = submit(
        &client,
        Some(route),
        Command::PendingPublications { after: 0 },
    )
    .wait()
    .unwrap();
    let p = match tickets.result() {
        Ok(Response::Publications(p)) if p.len() == 1 => p[0],
        x => panic!("{x:?}"),
    };
    assert_eq!(p.revision(), 1);
    drop(tickets);
    // Explicit caller notification of its actual reply-send attempt; observation
    // above did not settle it. Native kernel integration remains a separate proof.
    reply(&client, p);
    drop(submit(&client, Some(route), Command::Capture));
    let stop = Instant::now() + Duration::from_secs(2);
    while client.diagnostics().unwrap().outstanding != 0 {
        assert!(
            Instant::now() < stop,
            "lost capture completion kept resident credits"
        );
        std::thread::yield_now();
    }
    let retained = submit(&client, Some(route), Command::RetainedCapture)
        .wait()
        .unwrap();
    let capture = match retained.result() {
        Ok(Response::RetainedCapture(Some(c))) => *c,
        x => panic!("{x:?}"),
    };
    assert_eq!(capture.revision, 1);
    drop(retained);
    let later = write(&client, route, 3);
    let later_p = publication(&later);
    drop(later);
    let observed = submit(&client, Some(route), Command::RetainedCapture)
        .wait()
        .unwrap();
    assert!(matches!(observed.result(),Ok(Response::RetainedCapture(Some(c))) if *c==capture));
    drop(observed);
    let names = submit(
        &client,
        Some(route),
        Command::CapturedDentries {
            capture,
            after: None,
        },
    )
    .wait()
    .unwrap();
    let after = match names.result() {
        Ok(Response::Dentries(rows)) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].name, vec![0xff; 255]);
            assert!(client.diagnostics().unwrap().credited_bytes >= rows[0].name.capacity());
            Some((rows[0].parent, rows[0].name.clone()))
        }
        x => panic!("{x:?}"),
    };
    drop(names);
    let end = submit(
        &client,
        Some(route),
        Command::CapturedDentries { capture, after },
    )
    .wait()
    .unwrap();
    assert!(matches!(end.result(),Ok(Response::Dentries(rows)) if rows.is_empty()));
    drop(end);
    let invalid = submit(
        &client,
        Some(route),
        Command::CapturedDentries {
            capture,
            after: Some((1, vec![1; 256])),
        },
    )
    .wait()
    .unwrap();
    assert!(matches!(
        invalid.result(),
        Err(OwnerError::Overlay(OverlayError::Invalid(_)))
    ));
    drop(invalid);
    let wrong_route = submit(
        &client,
        Some(other),
        Command::CapturedCell {
            capture,
            serial: 2,
            offset: 0,
        },
    )
    .wait()
    .unwrap();
    assert!(matches!(
        wrong_route.result(),
        Err(OwnerError::Overlay(OverlayError::Stale))
    ));
    drop(wrong_route);
    assert!(submit(&client, Some(route), Command::Close)
        .wait()
        .unwrap()
        .result()
        .is_ok());
    let bytes = submit(
        &client,
        Some(route),
        Command::CapturedCell {
            capture,
            serial: 2,
            offset: 0,
        },
    )
    .wait()
    .unwrap();
    assert!(
        matches!(bytes.result(),Ok(Response::Cell(Some(cell))) if cell.data.iter().all(|b|*b==17))
    );
    assert!(client.diagnostics().unwrap().credited_bytes >= CELL_BYTES + MASK_BYTES);
    drop(bytes);
    assert!(submit(&client, Some(other), Command::State)
        .wait()
        .unwrap()
        .result()
        .is_ok());
    // No upstream construction/Save/history was entered in this operation. The
    // consumer is fenced, so explicit release has a known local-only disposition.
    assert!(
        submit(&client, Some(route), Command::ReleaseClosedCapture(capture))
            .wait()
            .unwrap()
            .result()
            .is_ok()
    );
    reply(&client, later_p);
    assert!(client.diagnostics().unwrap().peak_credited_bytes <= OwnerConfig::default().bytes);
    assert!(client.maintenance_failure().unwrap().is_none());
    owner.stop().unwrap();
}

#[test]
fn known_install_fences_later_sources_while_existing_reads_mutations_and_other_work_progress() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let route = open(&client, 130);
    let other = open(&client, 131);
    let done = write(&client, route, 2);
    reply(&client, publication(&done));
    drop(done);
    let captured = submit(&client, Some(route), Command::Capture)
        .wait()
        .unwrap();
    let capture = match captured.result() {
        Ok(Response::Captured(c)) => *c,
        x => panic!("{x:?}"),
    };
    drop(captured);
    let earlier = submit(
        &client,
        Some(route),
        Command::AcquireBaseSource { owner: 7 },
    );
    let install = submit(
        &client,
        Some(route),
        Command::Install {
            capture,
            root: [77; 32],
        },
    );
    let later = submit(
        &client,
        Some(route),
        Command::AcquireBaseSource { owner: 8 },
    );
    let earlier = earlier.wait().unwrap();
    let source = match earlier.result() {
        Ok(Response::BaseSource(s)) => *s,
        x => panic!("{x:?}"),
    };
    drop(earlier);
    assert_eq!(source.root(), [17; 32]);
    assert!(install.try_complete().unwrap().is_none());
    assert!(later.try_complete().unwrap().is_none());
    let read = submit(
        &client,
        Some(route),
        Command::SourceInode { source, serial: 2 },
    )
    .wait()
    .unwrap();
    assert!(matches!(read.result(),Ok(Response::Inode(Some(i))) if i.serial==2));
    drop(read);
    let mutation = write(&client, route, 3);
    let publication = publication(&mutation);
    drop(mutation);
    reply(&client, publication);
    assert!(submit(&client, Some(other), Command::State)
        .wait()
        .unwrap()
        .result()
        .is_ok());
    assert!(
        submit(&client, Some(route), Command::ReleaseBaseSource(source))
            .wait()
            .unwrap()
            .result()
            .is_ok()
    );
    assert!(install.wait().unwrap().result().is_ok());
    let later = later.wait().unwrap();
    let next = match later.result() {
        Ok(Response::BaseSource(s)) => *s,
        x => panic!("{x:?}"),
    };
    drop(later);
    assert_eq!(next.root(), [77; 32]);
    assert!(
        submit(&client, Some(route), Command::ReleaseBaseSource(next))
            .wait()
            .unwrap()
            .result()
            .is_ok()
    );
    let state = submit(&client, Some(route), Command::State).wait().unwrap();
    assert!(
        matches!(state.result(),Ok(Response::State(s)) if s.base_readers==0&&s.base_root==[77;32])
    );
    drop(state);
    assert!(
        client.diagnostics().unwrap().completed[layerfs_daemon::ServiceClass::Source as usize] >= 2
    );
    owner.stop().unwrap();
}

#[test]
fn lost_source_completion_retains_backed_owner_and_terminal_release_makes_cleanup_eligible() {
    use std::time::{Duration, Instant};
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let route = open(&client, 132);
    drop(submit(
        &client,
        Some(route),
        Command::AcquireBaseSource { owner: 19 },
    ));
    let stop = Instant::now() + Duration::from_secs(2);
    while client.diagnostics().unwrap().outstanding != 0 {
        assert!(Instant::now() < stop);
        std::thread::yield_now();
    }
    let retained = submit(
        &client,
        Some(route),
        Command::RetainedBaseSource { owner: 19 },
    )
    .wait()
    .unwrap();
    let source = match retained.result() {
        Ok(Response::RetainedBaseSource(Some(s))) => *s,
        x => panic!("{x:?}"),
    };
    drop(retained);
    assert!(submit(&client, Some(route), Command::Close)
        .wait()
        .unwrap()
        .result()
        .is_ok());
    let state = submit(&client, Some(route), Command::CleanupState)
        .wait()
        .unwrap();
    assert!(matches!(
        state.result(),
        Ok(Response::CleanupState(layerfs_overlay::CleanupState::Held))
    ));
    drop(state);
    assert!(
        submit(&client, Some(route), Command::ReleaseBaseSource(source))
            .wait()
            .unwrap()
            .result()
            .is_ok()
    );
    owner.stop().unwrap();
}
