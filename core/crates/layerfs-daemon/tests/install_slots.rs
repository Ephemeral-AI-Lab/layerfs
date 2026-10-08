//! R6-1 at owner scope: a queued known install and the source acquisitions it
//! holds back must leave the holder of the earlier base source a job slot.
//! Public owner API only; every wait is bounded and no thread is spawned.
use layerfs_daemon::{
    Command, Completion, Owner, OwnerClient, OwnerConfig, OwnerError, OwnerWork, Pending, Response,
    ServiceClass,
};
use layerfs_overlay::{BaseSource, Inode, InodeKind, ProfileConfig, Route};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(5);
const BASE: [u8; 32] = [17; 32];
const INSTALLED: [u8; 32] = [77; 32];

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("layerfs-install-slots-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn submit(client: &OwnerClient, route: Option<Route>, command: Command) -> Pending {
    match client.try_submit(route, command) {
        Ok(pending) => pending,
        Err((error, command)) => panic!("admission of {command:?}: {error:?}"),
    }
}
/// Bounded wait: a stalled owner fails the test instead of hanging it.
fn finish(client: &OwnerClient, pending: &Pending, what: &str) -> Completion {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(
            Instant::now() < deadline,
            "{what} did not complete within {WAIT:?}: {:?}",
            client.diagnostics().unwrap()
        );
        std::thread::yield_now();
    }
}
fn run(client: &OwnerClient, route: Option<Route>, command: Command, what: &str) -> Completion {
    finish(client, &submit(client, route, command), what)
}
/// A published result is visible before its publisher drops its own reference
/// to the credit, so a count is observed, never assumed. No job is issued.
fn outstanding(client: &OwnerClient, expected: usize, what: &str) -> OwnerWork {
    let deadline = Instant::now() + WAIT;
    loop {
        let work = client.diagnostics().unwrap();
        if work.outstanding == expected {
            return work;
        }
        assert!(
            Instant::now() < deadline,
            "{what}: outstanding never reached {expected}: {work:?}"
        );
        std::thread::yield_now();
    }
}
fn source(done: &Completion, what: &str) -> BaseSource {
    match done.result() {
        Ok(Response::BaseSource(source)) => *source,
        other => panic!("{what}: {other:?}"),
    }
}
fn acquire(owner: u64) -> Command {
    Command::AcquireBaseSource { owner }
}

#[test]
fn a_queued_install_and_the_sources_behind_it_leave_the_holder_a_slot() {
    let temp = Temp::new();
    let config = OwnerConfig::default();
    let jobs = config.jobs_per_namespace;
    assert_eq!((jobs, config.lifecycle_jobs_per_namespace), (16, 2));
    let owner = Owner::start(&temp.0.join("db"), ProfileConfig::default(), config).unwrap();
    let client = owner.client();
    let opened = run(
        &client,
        None,
        Command::Open {
            incarnation: [140; 32],
            base_root: BASE,
        },
        "open",
    );
    let route = match opened.result() {
        Ok(Response::Opened(route)) => *route,
        other => panic!("open: {other:?}"),
    };
    drop(opened);
    let lane = Some(route);

    // One published and replied inode, then the capture a known install needs.
    let written = run(
        &client,
        lane,
        Command::Publish {
            inode: Inode {
                serial: 2,
                kind: InodeKind::File,
                mode: 0o644,
                mtime_seconds: 1,
                mtime_nanoseconds: 2,
                nlink: 1,
                size: 0,
                inherited_cutoff: 0,
                born: 0,
                entries: 0,
            },
            name: None,
            cell: None,
        },
        "publish",
    );
    let publication = match written.result() {
        Ok(Response::Published(publication)) => *publication,
        other => panic!("publish: {other:?}"),
    };
    drop(written);
    let replied = run(
        &client,
        lane,
        Command::ReplyAttempted(publication),
        "reply attempt",
    );
    assert!(matches!(replied.result(), Ok(Response::Done)));
    drop(replied);
    let captured = run(&client, lane, Command::Capture, "capture");
    let capture = match captured.result() {
        Ok(Response::Captured(capture)) => *capture,
        other => panic!("capture: {other:?}"),
    };
    drop(captured);

    // The holder: one base source acquired, its completion dropped, so the
    // request owns a source in the engine and no owner credit.
    let granted = run(&client, lane, acquire(7), "holder's source");
    let held = source(&granted, "holder's source");
    assert_eq!(held.root(), BASE);
    drop(granted);
    let idle = outstanding(&client, 0, "before the install");

    // The install runs one readiness turn, finds the base reader and parks.
    let install = submit(
        &client,
        lane,
        Command::Install {
            capture,
            root: INSTALLED,
        },
    );
    let deadline = Instant::now() + WAIT;
    loop {
        let now = client.diagnostics().unwrap();
        if now.queued == 1
            && now.sql_foreground.total().attempts > idle.sql_foreground.total().attempts
        {
            break;
        }
        assert!(Instant::now() < deadline, "install never parked: {now:?}");
        std::thread::yield_now();
    }
    assert!(install.try_complete().unwrap().is_none());

    // Fifteen later acquisitions are held back behind the parked install: with
    // it, sixteen admitted jobs of this Workspace that cannot run.
    let mut later: Vec<Pending> = (0..jobs as u64 - 1)
        .map(|index| submit(&client, lane, acquire(100 + index)))
        .collect();
    outstanding(&client, jobs, "install and fifteen held-back sources");

    // (1) The holder's next job, the read a kernel request's observation
    // makes on its source, is admitted and completes with the install parked.
    let asked = match client.try_submit(
        lane,
        Command::SourceInode {
            source: held,
            serial: 2,
        },
    ) {
        Ok(pending) => pending,
        Err((error, _)) => panic!(
            "(1) the holder's Read job was refused {error:?} behind a parked install and {} \
             held-back sources; the install waits for this holder: {:?}",
            later.len(),
            client.diagnostics().unwrap()
        ),
    };
    let read = finish(&client, &asked, "(1) the holder's Read job");
    assert!(
        matches!(read.result(), Ok(Response::Inode(Some(inode))) if inode.serial == 2),
        "(1) the holder's read: {:?}",
        read.result()
    );
    // The pending handle owns the job's credit as much as its result does.
    drop(read);
    drop(asked);
    assert!(install.try_complete().unwrap().is_none());
    for pending in &later {
        assert!(pending.try_complete().unwrap().is_none());
    }

    // (2) The Source bound is its own: a sixteenth queued acquisition fills
    // it, the seventeenth is refused before any effect, and an ordinary job
    // of the same Workspace is admitted at that moment.
    later.push(submit(&client, lane, acquire(100 + jobs as u64 - 1)));
    assert_eq!(later.len(), jobs);
    assert!(matches!(
        client.try_submit(lane, acquire(999)),
        Err((
            OwnerError::AdmissionFull,
            Command::AcquireBaseSource { owner: 999 }
        ))
    ));
    let ordinary = run(&client, lane, Command::Inode(2), "(2) an ordinary job");
    assert!(matches!(
        ordinary.result(),
        Ok(Response::Inode(Some(inode))) if inode.serial == 2
    ));
    drop(ordinary);
    outstanding(&client, jobs + 1, "sixteen sources and the install");

    // The ordinary bound is unchanged beside it: the install and fifteen held
    // results fill it, a further ordinary job is refused, and the lifecycle
    // slots stay available.
    let results: Vec<Completion> = (1..jobs)
        .map(|_| run(&client, lane, Command::Inode(2), "a held ordinary result"))
        .collect();
    assert!(matches!(
        client.try_submit(lane, Command::Inode(2)),
        Err((OwnerError::AdmissionFull, Command::Inode(2)))
    ));
    assert!(matches!(
        client.try_submit(lane, acquire(999)),
        Err((OwnerError::AdmissionFull, _))
    ));
    let state = run(&client, lane, Command::State, "a lifecycle job");
    assert!(
        matches!(state.result(), Ok(Response::State(state)) if state.base_readers == 1 && state.base_root == BASE)
    );
    drop(state);
    let full = outstanding(&client, 2 * jobs, "both bounds full");
    assert!(full.credited_bytes <= config.bytes - config.lifecycle_reserve);
    drop(results);
    outstanding(&client, jobs + 1, "ordinary results released");
    assert!(install.try_complete().unwrap().is_none());

    // (3) The holder releases; the install runs, then every held-back
    // acquisition completes on the installed root.
    let released = run(
        &client,
        lane,
        Command::ReleaseBaseSource(held),
        "holder's release",
    );
    assert!(matches!(released.result(), Ok(Response::Done)));
    drop(released);
    let installed = finish(&client, &install, "(3) the install");
    assert!(
        matches!(installed.result(), Ok(Response::Done)),
        "(3) the install: {:?}",
        installed.result()
    );
    assert!(installed.work().parked_turns >= 1);
    drop(installed);
    drop(install);
    for pending in later {
        let granted = finish(&client, &pending, "(3) a held-back source");
        let next = source(&granted, "(3) a held-back source");
        assert_eq!(next.root(), INSTALLED);
        drop(granted);
        drop(pending);
        let released = run(
            &client,
            lane,
            Command::ReleaseBaseSource(next),
            "release of a held-back source",
        );
        assert!(matches!(released.result(), Ok(Response::Done)));
    }
    let state = run(&client, lane, Command::State, "final state");
    assert!(
        matches!(state.result(), Ok(Response::State(state)) if state.base_readers == 0 && state.base_root == INSTALLED)
    );
    drop(state);

    // (4) Every credit returned and no fixed bound was exceeded.
    let end = outstanding(&client, 0, "end");
    assert_eq!((end.credited_bytes, end.queued), (0, 0));
    assert!(end.peak_credited_bytes <= config.bytes);
    assert!(end.peak_queued <= 2 * jobs + config.lifecycle_jobs_per_namespace);
    assert_eq!(
        end.completed[ServiceClass::Source as usize],
        1 + jobs as u64
    );
    assert!(client.maintenance_failure().unwrap().is_none());
    println!(
        "R6_1_OWNER held_back_sources={} source_bound={jobs} ordinary_bound={jobs} \
         peak_outstanding={} peak_queued={} peak_credited_bytes={} scheduler_bytes={} \
         completed_source={}",
        jobs,
        full.outstanding,
        end.peak_queued,
        end.peak_credited_bytes,
        end.scheduler_bytes,
        end.completed[ServiceClass::Source as usize]
    );
    owner.stop().unwrap();
}
