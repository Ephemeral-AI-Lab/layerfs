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
        subdirs: 0,
    }
}

#[test]
fn failed_capture_waits_for_readers_while_later_writes_and_idle_composition_progress() {
    use layerfs_overlay::{Lease, LeaseKind};
    use std::time::{Duration, Instant};
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("failure"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let route = open(&client, 125);
    let wait = |pending: Pending| {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "owner progress deadline");
            if let Some(done) = pending.try_complete().unwrap() {
                return done;
            }
            std::thread::yield_now();
        }
    };
    for serial in 1..=256 {
        let done = write(&client, route, serial);
        reply(&client, publication(&done));
    }
    let done = wait(submit(&client, Some(route), Command::Capture));
    let capture = match done.result() {
        Ok(Response::Captured(c)) => *c,
        x => panic!("{x:?}"),
    };
    drop(done);
    let reader = Lease {
        kind: LeaseKind::Reader,
        owner: 901,
        resource: capture.generation.number() as u64,
    };
    assert!(wait(submit(&client, Some(route), Command::Acquire(reader)))
        .result()
        .is_ok());
    assert!(wait(submit(
        &client,
        Some(route),
        Command::ResolveFailed(capture)
    ))
    .result()
    .is_ok());
    let next = submit(&client, Some(route), Command::Capture);
    // A next capture is parked for composition, not a payload-sized mutation
    // fence. Later writes must finish while the lower reader remains owned.
    let done = wait(submit(
        &client,
        Some(route),
        Command::Publish {
            inode: value(999),
            name: None,
            cell: None,
        },
    ));
    reply(&client, publication(&done));
    drop(done);
    assert!(next.try_complete().unwrap().is_none());
    assert!(wait(submit(&client, Some(route), Command::Release(reader)))
        .result()
        .is_ok());
    // No additional write/status/cleanup job triggers the composition.
    let done = wait(next);
    let captured = match done.result() {
        Ok(Response::Captured(c)) => *c,
        x => panic!("{x:?}"),
    };
    drop(done);
    let done = wait(submit(
        &client,
        Some(route),
        Command::CapturedInodes {
            capture: captured,
            after: 998,
        },
    ));
    assert!(
        matches!(done.result(),Ok(Response::Inodes(rows)) if rows.len()==1 && rows[0].serial==999)
    );
    drop(done);
    assert!(client.maintenance_failure().unwrap().is_none());
    println!(
        "S6_OWNER_FAILURE automatic_work={:?}",
        client.diagnostics().unwrap()
    );
    owner.stop().unwrap();
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
    assert_eq!(owner.profile().schema_version, 24);
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
    use layerfs_overlay::{Cell, DirectoryEntry, OverlayError, CELL_BYTES, MASK_BYTES};
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
            name: Some(DirectoryEntry {
                inherited: false,
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
        Command::CapturedDirectoryEntries {
            capture,
            after: None,
        },
    )
    .wait()
    .unwrap();
    let after = match names.result() {
        Ok(Response::DirectoryEntries(rows)) => {
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
        Command::CapturedDirectoryEntries { capture, after },
    )
    .wait()
    .unwrap();
    assert!(matches!(end.result(),Ok(Response::DirectoryEntries(rows)) if rows.is_empty()));
    drop(end);
    let invalid = submit(
        &client,
        Some(route),
        Command::CapturedDirectoryEntries {
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

/// Bounded wait for one admitted job: a stalled owner fails the test instead
/// of hanging it.
fn done(pending: &Pending, what: &str) -> Completion {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{what}: no completion"
        );
        std::thread::yield_now();
    }
}
fn run(client: &OwnerClient, route: Route, command: Command, what: &str) -> Completion {
    done(&submit(client, Some(route), command), what)
}
/// Credits are returned after the result is visible: observe, bounded.
fn settled(client: &OwnerClient, outstanding: usize) -> layerfs_daemon::OwnerWork {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let work = client.diagnostics().unwrap();
        if work.outstanding == outstanding {
            return work;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "credits: outstanding={} queued={} admitted={} completed={:?}",
            work.outstanding,
            work.queued,
            work.admitted,
            work.completed
        );
        std::thread::yield_now();
    }
}
fn pending_publications(client: &OwnerClient, route: Route) -> Vec<Publication> {
    let done = run(
        client,
        route,
        Command::PendingPublications { after: 0 },
        "pending publications",
    );
    match done.result() {
        Ok(Response::Publications(pending)) => pending.clone(),
        x => panic!("pending publications: {x:?}"),
    }
}
fn cleanup_state(client: &OwnerClient, route: Route) -> layerfs_overlay::CleanupState {
    let done = run(client, route, Command::CleanupState, "cleanup state");
    match done.result() {
        Ok(Response::CleanupState(state)) => *state,
        x => panic!("cleanup state: {x:?}"),
    }
}
/// `Owner::stop` joins the owner thread. The join runs on a helper thread
/// whose exit depends on nothing of the test, and the test waits bounded.
fn stop_bounded(owner: Owner) {
    let (send, stopped) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(owner.stop());
    });
    match stopped.recv_timeout(std::time::Duration::from_secs(20)) {
        Ok(result) => result.unwrap(),
        Err(_) => panic!("the owner did not stop"),
    }
}

#[test]
fn a_reply_attempt_is_recorded_without_an_owner_job_and_a_parked_capture_is_owed_one() {
    use layerfs_daemon::ServiceClass;
    use layerfs_overlay::{OverlayError, Settled};
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("overlay"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let a = open(&client, 21);
    let b = open(&client, 22);
    let publish = |route: Route, serial: u64| {
        let done = run(
            &client,
            route,
            Command::Publish {
                inode: value(serial),
                name: None,
                cell: None,
            },
            "publish",
        );
        publication(&done)
    };
    let first = publish(a, 2);
    let second = publish(a, 3);
    let other = publish(b, 2);
    assert_eq!(pending_publications(&client, a), vec![first, second]);
    assert_eq!(pending_publications(&client, b), vec![other]);

    // No waiter: attempts are recorded from any thread and admit no job.
    let before = settled(&client, 0);
    assert_eq!(client.reply_attempted(other).unwrap(), Settled::Last);
    assert!(matches!(
        client.reply_attempted(other),
        Err(OwnerError::Overlay(OverlayError::Stale))
    ));
    let replying = client.clone();
    let from_thread = std::thread::spawn(move || replying.reply_attempted(first))
        .join()
        .unwrap();
    assert_eq!(from_thread.unwrap(), Settled::Pending);
    let work = client.diagnostics().unwrap();
    assert_eq!(
        (work.admitted, work.completed, work.outstanding, work.queued),
        (before.admitted, before.completed, 0, 0)
    );
    // The owner sees what the threads recorded.
    assert_eq!(pending_publications(&client, a), vec![second]);
    assert!(pending_publications(&client, b).is_empty());

    // A capture parks behind the last ticket: its readiness turn's read is
    // published before the job is requeued.
    let before = settled(&client, 0);
    let capture = submit(&client, Some(a), Command::Capture);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let work = client.diagnostics().unwrap();
        if work.queued == 1
            && work.sql_foreground.total().attempts > before.sql_foreground.total().attempts
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "capture never parked");
        std::thread::yield_now();
    }
    assert!(capture.try_complete().unwrap().is_none());
    // Another Workspace's ticket traffic neither settles nor wakes it.
    let unrelated = publish(b, 3);
    assert_eq!(client.reply_attempted(unrelated).unwrap(), Settled::Last);
    assert!(capture.try_complete().unwrap().is_none());

    // The last attempt is told that an owner job waited. Recording it is
    // still no owner job, so the capture stays parked until the one
    // settling job the caller now owes has run.
    let parked = settled(&client, 1);
    assert_eq!(client.reply_attempted(second).unwrap(), Settled::Watched);
    let work = client.diagnostics().unwrap();
    assert_eq!(
        (work.admitted, work.completed, work.queued),
        (parked.admitted, parked.completed, 1)
    );
    assert!(capture.try_complete().unwrap().is_none());
    let settling = run(&client, a, Command::ReplySettled, "reply settled");
    assert!(matches!(settling.result(), Ok(Response::Done)));
    // A live Workspace has nothing to queue: the job wrote nothing.
    assert_eq!(settling.work().sql.total().rows_changed, 0);
    drop(settling);
    let captured = done(&capture, "parked capture");
    let sealed = match captured.result() {
        Ok(Response::Captured(capture)) => *capture,
        x => panic!("capture: {x:?}"),
    };
    assert!(captured.work().parked_turns >= 1);
    assert_eq!(sealed.revision, second.revision());
    // The result's credit is held by its completion and by its handle.
    drop((captured, capture));
    let work = settled(&client, 0);
    let mut completed = parked.completed;
    completed[ServiceClass::Lifecycle as usize] += 1;
    completed[ServiceClass::Capture as usize] += 1;
    assert_eq!(
        (work.admitted, work.completed, work.queued),
        (parked.admitted + 1, completed, 0)
    );
    stop_bounded(owner);
}

#[test]
fn a_closed_workspace_is_reclaimed_only_after_the_settling_job_of_its_last_reply_attempt() {
    use layerfs_overlay::{CleanupState, Settled};
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("overlay"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let route = open(&client, 31);
    let published = write(&client, route, 2);
    let ticket = publication(&published);
    drop(published);
    assert!(matches!(
        run(&client, route, Command::Close, "close").result(),
        Ok(Response::Done)
    ));
    // The pending ticket holds the closed Workspace. Idle maintenance and
    // further owner turns have nothing queued to reclaim.
    for _ in 0..16 {
        assert_eq!(cleanup_state(&client, route), CleanupState::Held);
    }
    assert_eq!(client.diagnostics().unwrap().closed_namespaces, 0);
    // The close observed the ticket pending, so its attempt owes a turn.
    assert_eq!(client.reply_attempted(ticket).unwrap(), Settled::Watched);
    // The attempt alone queued nothing.
    for _ in 0..16 {
        assert_eq!(cleanup_state(&client, route), CleanupState::Held);
    }
    assert_eq!(client.diagnostics().unwrap().closed_namespaces, 0);
    assert!(matches!(
        run(&client, route, Command::ReplySettled, "reply settled").result(),
        Ok(Response::Done)
    ));
    // Queued by the settling job, then reclaimed by automatic maintenance.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        match cleanup_state(&client, route) {
            CleanupState::Gone => break,
            CleanupState::Queued => {}
            state => panic!("after the settling job: {state:?}"),
        }
        assert!(
            std::time::Instant::now() < deadline,
            "automatic cleanup did not finish"
        );
        std::thread::yield_now();
    }
    assert_eq!(client.diagnostics().unwrap().closed_namespaces, 1);
    assert!(client.maintenance_failure().unwrap().is_none());
    stop_bounded(owner);
}

#[test]
fn concurrent_submitters_get_every_job_done_exactly_once_with_its_own_result() {
    use layerfs_daemon::ServiceClass;
    use layerfs_overlay::Settled;
    const THREADS: u64 = 4;
    const ROUNDS: u64 = 300;
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("overlay"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    // Two namespaces, each shared by two submitting threads, so that turns
    // collide both inside a lane and across the rotation.
    let routes = [open(&client, 41), open(&client, 42)];
    let before = settled(&client, 0);

    // Each thread makes its own bounded waits and never waits for another
    // thread: one that fails ends by itself and the scope reports it.
    let published: Vec<Vec<Publication>> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..THREADS)
            .map(|thread| {
                let client = client.clone();
                let route = routes[(thread % 2) as usize];
                scope.spawn(move || {
                    let mut published = Vec::with_capacity(ROUNDS as usize);
                    for round in 0..ROUNDS {
                        // A serial no other thread or round uses.
                        let serial = 2 + thread * ROUNDS + round;
                        let wrote = run(
                            &client,
                            route,
                            Command::Publish {
                                inode: value(serial),
                                name: None,
                                cell: None,
                            },
                            "publish",
                        );
                        let ticket = publication(&wrote);
                        drop(wrote);
                        assert_eq!(ticket.route(), route);
                        // Its own result: the row this thread just wrote.
                        let read = run(&client, route, Command::Inode(serial), "inode");
                        assert!(
                            matches!(read.result(), Ok(Response::Inode(Some(row))) if *row == value(serial)),
                            "thread {thread} round {round}: {:?}",
                            read.result()
                        );
                        drop(read);
                        // The reply attempt is recorded from this thread
                        // and nothing waits for it.
                        let attempt = client.reply_attempted(ticket).unwrap();
                        assert_ne!(attempt, Settled::Watched);
                        published.push(ticket);
                    }
                    published
                })
            })
            .collect();
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect()
    });

    // Every publishing job ran exactly once: each namespace's revisions are
    // the dense sequence of its jobs, and each thread saw its own ascend.
    for (index, route) in routes.iter().enumerate() {
        let mut revisions = Vec::new();
        for (thread, tickets) in published.iter().enumerate() {
            if thread % 2 != index {
                continue;
            }
            assert_eq!(tickets.len(), ROUNDS as usize);
            assert!(tickets
                .windows(2)
                .all(|pair| pair[0].revision() < pair[1].revision()));
            revisions.extend(tickets.iter().map(|ticket| ticket.revision()));
        }
        revisions.sort_unstable();
        let expected: Vec<i64> = (1..=(THREADS / 2 * ROUNDS) as i64).collect();
        assert_eq!(revisions, expected, "namespace {index}");
        let state = run(&client, *route, Command::State, "state");
        assert!(
            matches!(state.result(), Ok(Response::State(state)) if state.revision == expected.len() as i64)
        );
        drop(state);
        assert!(pending_publications(&client, *route).is_empty());
    }

    // The owner's totals match what was submitted: every admitted job
    // completed, in its own class, and no reply attempt was a job.
    let jobs = THREADS * ROUNDS;
    let after = settled(&client, 0);
    // The two State and two PendingPublications observations above.
    assert_eq!(after.admitted, before.admitted + 2 * jobs + 4);
    assert_eq!(
        after.completed.iter().sum::<u64>(),
        before.completed.iter().sum::<u64>() + 2 * jobs + 4
    );
    let class =
        |class: ServiceClass| after.completed[class as usize] - before.completed[class as usize];
    assert_eq!(class(ServiceClass::Mutation), jobs);
    assert_eq!(class(ServiceClass::Read), jobs + 2);
    assert_eq!(class(ServiceClass::Lifecycle), 2);
    assert_eq!(after.admitted, after.completed.iter().sum::<u64>());
    assert_eq!((after.queued, after.credited_bytes), (0, 0));
    assert!(client.maintenance_failure().unwrap().is_none());
    println!(
        "OWNER-CONTENTION threads={THREADS} namespaces=2 jobs={} admitted==completed=true peak_queued={} reply_attempt_jobs=0",
        2 * jobs,
        after.peak_queued
    );

    // The owner stops cleanly and admits nothing afterwards.
    stop_bounded(owner);
    assert!(matches!(
        client.try_submit(Some(routes[0]), Command::State),
        Err((OwnerError::Stopped, _))
    ));
}
