//! Public S4 proofs through the real daemon owner: every namespace round is an
//! actual fair owner job, with provider demand outside it and real threads.
mod common;
mod harness;
use common::{fixture, name, Store};
use harness::{create, link, mkdir, path, rename, rmdir, unlink, Allocator, T1, T2};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_daemon::{Command, Owner, OwnerClient, OwnerConfig, OwnerError, Response};
use layerfs_overlay::{ProfileConfig, Route};
use layerfs_workspace::{
    BaseView, CanonicalClient, Operation, Outcome, OverlayRead, Refusal, Time, ViewStat, Workspace,
    WorkspaceError, WorkspaceResult,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Barrier, Condvar, Mutex,
    },
    thread,
};

/// Blocks the first armed read of one object until released.
struct Gate {
    store: Store,
    object: ObjectId,
    state: Mutex<(bool, bool)>,
    wake: Condvar,
    entered: mpsc::SyncSender<()>,
}
impl AuthenticatedObjects for Gate {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let mut state = self.state.lock().unwrap();
        if ids.contains(&self.object) && state.0 {
            state.0 = false;
            self.entered.send(()).unwrap();
            while !state.1 {
                let waited = self
                    .wake
                    .wait_timeout(state, std::time::Duration::from_secs(10))
                    .unwrap();
                state = waited.0;
                assert!(!waited.1.timed_out(), "base gate deadline");
            }
        }
        drop(state);
        self.store.read_canonical_batch(ids)
    }
}
struct Service {
    owner: Option<Owner>,
    client: OwnerClient,
    route: Route,
    workspace: Workspace,
    allocator: Allocator,
    owners: AtomicU64,
    gate: Arc<Gate>,
    waiting: Mutex<mpsc::Receiver<()>>,
    path: PathBuf,
}
impl Drop for Service {
    fn drop(&mut self) {
        drop(self.owner.take());
        let _ = std::fs::remove_file(&self.path);
    }
}
impl Service {
    fn new(tag: &str) -> Self {
        let f = fixture();
        let (entered, waiting) = mpsc::sync_channel(1);
        let gate = Arc::new(Gate {
            store: f.store.clone(),
            object: f.root.0,
            state: Mutex::new((false, false)),
            wake: Condvar::new(),
            entered,
        });
        let client = Arc::new(CanonicalClient::with_lengths(
            gate.clone(),
            Arc::new(f.store.clone()),
            0,
        ));
        let base = BaseView::open(client, f.root, f.scope).unwrap();
        let path = std::env::temp_dir().join(format!(
            "layerfs-namespace-daemon-{tag}-{}.sqlite",
            std::process::id()
        ));
        let owner = Owner::start(&path, ProfileConfig::default(), OwnerConfig::default()).unwrap();
        let client = owner.client();
        let opened = client
            .try_submit(
                None,
                Command::Open {
                    incarnation: [77; 32],
                    base_root: f.root.0.to_bytes(),
                },
            )
            .unwrap_or_else(|(error, _)| panic!("{error:?}"))
            .wait()
            .unwrap();
        let route = match opened.result() {
            Ok(Response::Opened(route)) => *route,
            other => panic!("{other:?}"),
        };
        drop(opened);
        Self {
            owner: Some(owner),
            client,
            route,
            workspace: Workspace::bind(route, base),
            allocator: Allocator {
                next: AtomicU64::new(1000),
                calls: AtomicU64::new(0),
            },
            owners: AtomicU64::new(0),
            gate,
            waiting: Mutex::new(waiting),
            path,
        }
    }
    /// Submits one owner job. Admission refusal precedes any attempt and
    /// returns the original command, so waiting for credits is not a replay.
    fn job<T>(&self, mut command: Command, read: impl FnOnce(&Response) -> T) -> T {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let pending = loop {
            assert!(std::time::Instant::now() < deadline, "admission deadline");
            match self.client.try_submit(Some(self.route), command) {
                Ok(pending) => break pending,
                Err((OwnerError::AdmissionFull, original)) => {
                    command = original;
                    thread::yield_now();
                }
                Err((error, _)) => panic!("{error:?}"),
            }
        };
        let done = loop {
            assert!(std::time::Instant::now() < deadline, "owner job deadline");
            if let Some(done) = pending.try_complete().unwrap() {
                break done;
            }
            thread::yield_now();
        };
        match done.result() {
            Ok(response) => read(response),
            Err(error) => panic!("{error:?}"),
        }
    }
    fn window<T>(&self, work: impl FnOnce(&layerfs_workspace::SourceView) -> T) -> T {
        let owner = self.owners.fetch_add(1, Ordering::Relaxed) + 1;
        let source = self.job(Command::AcquireBaseSource { owner }, |r| match r {
            Response::BaseSource(source) => *source,
            other => panic!("{other:?}"),
        });
        let view = self.workspace.view_for_source(source).unwrap();
        let result = work(&view);
        self.job(Command::ReleaseBaseSource(source), |_| ());
        result
    }
    fn run(&self, operation: Operation, now: Time) -> WorkspaceResult<Outcome> {
        let outcome = self.window(|view| {
            self.workspace
                .mutate(&self.client, &self.allocator, view, operation, now)
        });
        if let Ok(Outcome::Applied { publication, .. }) = &outcome {
            self.job(Command::ReplyAttempted(*publication), |_| ());
        }
        outcome
    }
    fn applied(&self, operation: Operation) -> Option<ViewStat> {
        match self.run(operation, T1).unwrap() {
            Outcome::Applied { stat, .. } => stat,
            other => panic!("{other:?}"),
        }
    }
    fn lookup(&self, parent: u64, child: &str) -> Option<ViewStat> {
        match self.window(|view| view.lookup(&self.client, parent, &name(child))) {
            Ok(stat) => Some(stat),
            Err(WorkspaceError::Content(ContentError::PathNotFound)) => None,
            Err(error) => panic!("{error:?}"),
        }
    }
    fn names(&self, parent: u64) -> Vec<String> {
        let mut all = Vec::new();
        let mut after: Option<Vec<u8>> = None;
        loop {
            let page = self
                .window(|view| view.list(&self.client, parent, after.as_deref()))
                .unwrap();
            assert!(page.visited <= 64);
            all.extend(
                page.entries
                    .into_iter()
                    .map(|(name, _)| String::from_utf8(name).unwrap()),
            );
            match page.continuation {
                Some(next) => after = Some(next),
                None => return all,
            }
        }
    }
}
fn refusal(outcome: WorkspaceResult<Outcome>) -> Option<Refusal> {
    match outcome {
        Ok(_) => None,
        Err(WorkspaceError::Refused(refusal)) => Some(refusal),
        Err(error) => panic!("{error:?}"),
    }
}

#[test]
fn concurrent_operations_through_real_owner_jobs_keep_counts_and_references_exact() {
    const THREADS: usize = 4;
    const FILES: usize = 120;
    const LINKS: usize = 20;
    let service = Service::new("concurrent");
    let shared = service.applied(mkdir(1, "shared")).unwrap().serial;
    let start = Barrier::new(THREADS);
    let winners = AtomicU64::new(0);
    thread::scope(|scope| {
        for t in 0..THREADS {
            let (service, start, winners) = (&service, &start, &winners);
            scope.spawn(move || {
                start.wait();
                match refusal(service.run(create(shared, "contested"), T1)) {
                    None => {
                        winners.fetch_add(1, Ordering::Relaxed);
                    }
                    Some(refused) => assert_eq!(refused, Refusal::Exists),
                }
                for n in 0..FILES {
                    service.applied(create(shared, &format!("t{t}-f{n:03}")));
                    if n < LINKS {
                        service.applied(link(2, shared, &format!("t{t}-l{n:03}")));
                    }
                }
            });
        }
    });
    assert_eq!(winners.load(Ordering::Relaxed), 1, "one creator of a name");
    let names = service.names(shared);
    assert_eq!(names.len(), THREADS * (FILES + LINKS) + 1);
    // Lost updates would corrupt these maintained values; they are exact.
    let file = service.lookup(1, "file").unwrap();
    assert_eq!(file.namespace_refs, 2 + (THREADS * LINKS) as u64);
    assert_eq!(
        refusal(service.run(rmdir(1, "shared"), T2)),
        Some(Refusal::NotEmpty)
    );
    thread::scope(|scope| {
        for chunk in names.chunks(names.len().div_ceil(THREADS)) {
            let service = &service;
            scope.spawn(move || {
                for name in chunk {
                    service.applied(unlink(shared, name));
                }
            });
        }
    });
    assert_eq!(service.lookup(1, "alias").unwrap().namespace_refs, 2);
    service.applied(rmdir(1, "shared"));
    assert_eq!(service.lookup(1, "shared"), None);
    let work = service.client.diagnostics().unwrap();
    assert!(
        work.completed[1] >= (2 * names.len()) as u64,
        "mutation-class jobs"
    );
    assert_eq!(work.outstanding, 0);
    assert!(service.client.maintenance_failure().unwrap().is_none());
}

#[test]
fn opposing_directory_moves_from_real_threads_never_form_a_cycle() {
    let service = Service::new("cycles");
    for round in 0..40 {
        let (left, right) = (format!("left-{round}"), format!("right-{round}"));
        let a = service.applied(mkdir(1, &left)).unwrap().serial;
        let b = service.applied(mkdir(1, &right)).unwrap().serial;
        let start = Barrier::new(2);
        let (first, second) = thread::scope(|scope| {
            let into_right = scope.spawn(|| {
                start.wait();
                refusal(service.run(rename((1, &left), (b, "in"), true, path(&[&right])), T1))
            });
            let into_left = scope.spawn(|| {
                start.wait();
                refusal(service.run(rename((1, &right), (a, "in"), true, path(&[&left])), T1))
            });
            (into_right.join().unwrap(), into_left.join().unwrap())
        });
        // Exactly one move is published; the other's ancestry no longer holds.
        match (first, second) {
            (None, Some(Refusal::AncestryMismatch)) => {
                assert_eq!(service.lookup(1, &right).unwrap().serial, b);
                assert_eq!(service.lookup(b, "in").unwrap().serial, a);
                assert_eq!(service.lookup(1, &left), None);
            }
            (Some(Refusal::AncestryMismatch), None) => {
                assert_eq!(service.lookup(1, &left).unwrap().serial, a);
                assert_eq!(service.lookup(a, "in").unwrap().serial, b);
                assert_eq!(service.lookup(1, &right), None);
            }
            other => panic!("round {round}: {other:?}"),
        }
    }
}

#[test]
fn a_blocked_base_fact_leaves_the_owner_free_and_publishes_after_a_capture() {
    let service = Service::new("gated");
    // The first fact round of `gated` asks for base facts; its provider read
    // then blocks outside the owner.
    service.gate.state.lock().unwrap().0 = true;
    thread::scope(|scope| {
        let blocked = scope.spawn(|| service.applied(create(1, "gated")).unwrap());
        service.waiting.lock().unwrap().recv().unwrap();
        // The same Workspace keeps publishing complete operations meanwhile.
        let free = service.applied(create(1, "free")).unwrap();
        let dir = service.applied(mkdir(1, "dir")).unwrap();
        service.applied(create(dir.serial, "inner"));
        assert_eq!(service.lookup(1, "gated"), None);
        // A capture seals what is published; the blocked operation has
        // published nothing and holds no ticket, so it does not delay it.
        let capture = service.job(Command::Capture, |r| match r {
            Response::Captured(capture) => *capture,
            other => panic!("{other:?}"),
        });
        {
            let mut state = service.gate.state.lock().unwrap();
            state.1 = true;
            service.gate.wake.notify_all();
        }
        let gated = blocked.join().unwrap();
        let sealed = service.job(
            Command::CapturedDentries {
                capture,
                after: None,
            },
            |r| match r {
                Response::Dentries(rows) => rows
                    .iter()
                    .map(|row| String::from_utf8(row.name.clone()).unwrap())
                    .collect::<Vec<_>>(),
                other => panic!("{other:?}"),
            },
        );
        assert_eq!(sealed, vec!["dir", "free", "inner"]);
        // Its one publication landed in the generation current at its attempt.
        let born = |serial| {
            service.job(Command::Inode(serial), |r| match r {
                Response::Inode(Some(inode)) => inode.born,
                other => panic!("{other:?}"),
            })
        };
        assert_eq!(born(free.serial), capture.generation.number() as u64);
        assert_eq!(born(gated.serial), capture.generation.number() as u64 + 1);
        assert_eq!(service.lookup(1, "gated").unwrap().serial, gated.serial);
    });
}

#[test]
fn closed_and_stopped_services_return_exact_outcomes_without_effect() {
    let mut service = Service::new("closed");
    let dir = service.applied(mkdir(1, "dir")).unwrap().serial;
    // A source acquired before logical close keeps its reads; mutation through
    // it is refused by the one attempted publication with the engine's cause.
    let source = service.job(Command::AcquireBaseSource { owner: 900 }, |r| match r {
        Response::BaseSource(source) => *source,
        other => panic!("{other:?}"),
    });
    let view = service.workspace.view_for_source(source).unwrap();
    service.job(Command::Close, |_| ());
    let refused = service
        .workspace
        .mutate(
            &service.client,
            &service.allocator,
            &view,
            create(dir, "late"),
            T1,
        )
        .unwrap_err();
    let WorkspaceError::Service(cause) = &refused else {
        panic!("{refused:?}");
    };
    assert!(
        format!("{cause:?}").contains("Workspace(Overlay(Closed))"),
        "{cause:?}"
    );
    assert_eq!(view.stat(&service.client, dir).unwrap().serial, dir);
    assert!(matches!(
        view.lookup(&service.client, dir, &name("late")),
        Err(WorkspaceError::Content(ContentError::PathNotFound))
    ));
    // Whole-service stop returns the original unattempted command.
    service.owner.take().unwrap().stop().unwrap();
    let stopped = service
        .workspace
        .mutate(
            &service.client,
            &service.allocator,
            &view,
            create(dir, "never"),
            T2,
        )
        .unwrap_err();
    let text = format!("{stopped:?}");
    assert!(
        text.contains("Unattempted { cause: Stopped, command: Namespace(NamespaceJob"),
        "{text}"
    );
    let _ = OverlayRead::inode(&service.client, source, dir).unwrap_err();
}

// The original test used width 24 for 23-byte records. Writer panics left the
// tail spinning; retain both the correct width and panic-safe stop custody.
#[test]
fn concurrent_appenders_and_a_tail_reader_see_whole_records_through_real_owner_jobs() {
    const THREADS: usize = 4;
    const RECORDS: usize = 250;
    const WIDTH: usize = 23;
    let service = Service::new("append");
    let log = service.applied(create(1, "build.log")).unwrap().serial;
    let content = |service: &Service| {
        let mut bytes = Vec::new();
        service
            .window(|view| view.read(&service.client, log, 0, 128 * 1024, &mut bytes))
            .unwrap();
        bytes
    };
    let done = std::sync::atomic::AtomicBool::new(false);
    // Set on every exit, including a writer panic, so the tail cannot spin on.
    struct Stop<'a>(&'a std::sync::atomic::AtomicBool);
    impl Drop for Stop<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let snapshots = thread::scope(|scope| {
        let _stop = Stop(&done);
        // The tail reader takes whole-file windows while the writers append.
        let tail = scope.spawn(|| {
            let (mut last, mut taken) = (0, 0_u64);
            while !done.load(Ordering::Relaxed) {
                assert!(std::time::Instant::now() < deadline, "tail deadline");
                let bytes = content(&service);
                assert!(bytes.len() >= last, "a tail never sees the file shrink");
                assert_eq!(bytes.len() % WIDTH, 0, "only whole records are visible");
                assert!(bytes.chunks(WIDTH).all(|record| record.ends_with(b"\n")));
                last = bytes.len();
                taken += 1;
            }
            taken
        });
        let writers: Vec<_> = (0..THREADS)
            .map(|t| {
                let service = &service;
                scope.spawn(move || {
                    for n in 0..RECORDS {
                        let record = format!("writer-{t}-record-{n:06}\n");
                        assert_eq!(record.len(), WIDTH);
                        let stat = service
                            .applied(Operation::Write {
                                serial: log,
                                position: layerfs_workspace::Position::End,
                                data: record.as_bytes().into(),
                            })
                            .unwrap();
                        assert_eq!(stat.logical_len % WIDTH as u64, 0);
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        done.store(true, Ordering::Relaxed);
        tail.join().unwrap()
    });
    // Every record landed once, each writer's in its own order, none split.
    let bytes = content(&service);
    assert_eq!(bytes.len(), THREADS * RECORDS * WIDTH);
    let mut next = [0_usize; THREADS];
    for record in bytes.chunks(WIDTH) {
        let text = std::str::from_utf8(record).unwrap();
        let writer: usize = text[7..8].parse().unwrap();
        let number: usize = text[16..22].parse().unwrap();
        assert_eq!(number, next[writer], "{text}");
        next[writer] += 1;
    }
    assert_eq!(next, [RECORDS; THREADS]);
    assert!(snapshots > 0);
    assert_eq!(
        service.lookup(1, "build.log").unwrap().logical_len,
        (THREADS * RECORDS * WIDTH) as u64
    );
}

#[test]
fn payload_complete_operations_have_indexed_work_through_the_real_owner() {
    use layerfs_overlay::DatabaseWork;
    let service = Service::new("payload-profile");
    let file = service.applied(create(1, "profile.bin")).unwrap().serial;
    service.applied(Operation::Write {
        serial: file,
        position: layerfs_workspace::Position::At(0),
        data: vec![7; 131072].into(),
    });
    let plans = service.window(|view| {
        service.job(
            Command::PayloadPlans(view.source()),
            |response| match response {
                Response::PayloadPlans(plans) => plans.clone(),
                other => panic!("{other:?}"),
            },
        )
    });
    assert!(plans
        .iter()
        .all(|plan| plan.contains("SEARCH") && !plan.contains("SCAN") && !plan.contains("TEMP")));
    println!("S5_OWNER_PLANS {plans:?}");
    let snapshot = || {
        service.job(Command::DatabaseWork, |response| match response {
            Response::DatabaseWork(work) => **work,
            other => panic!("{other:?}"),
        })
    };
    let profile = |label: &str, scale, work: &dyn Fn()| {
        let before: DatabaseWork = snapshot();
        work();
        let after = snapshot();
        let mut totals = [0; 4];
        for (a, b) in before.statements.iter().zip(after.statements) {
            assert_eq!(
                (a.fullscan_steps, a.sorts, a.autoindex_rows, a.reprepares),
                (b.fullscan_steps, b.sorts, b.autoindex_rows, b.reprepares)
            );
            for (total, delta) in totals.iter_mut().zip([
                b.executions - a.executions,
                b.vm_steps - a.vm_steps,
                b.rows_changed - a.rows_changed,
                b.bound_bytes - a.bound_bytes,
            ]) {
                *total += delta;
            }
        }
        println!("S5_OWNER_OPERATION unrelated={scale} operation={label} statements={} vm={} changed={} bound_bytes={} fullscan=0 sorts=0 autoindex=0 reprepare=0 includes_post_observation_route_seek=true", totals[0], totals[1], totals[2], totals[3]);
        totals
    };
    let (mut filled, mut reference) = (0, None);
    for scale in [128, 1024, 4096] {
        while filled < scale {
            service.applied(create(1, &format!("other-{filled:05}")));
            filled += 1;
        }
        // Each scale starts with the same file history. Reusing the prior
        // mutated file would add a shrink step and confound unrelated growth.
        let file = service
            .applied(create(1, &format!("profile-{scale}")))
            .unwrap()
            .serial;
        service.applied(Operation::Write {
            serial: file,
            position: layerfs_workspace::Position::At(0),
            data: vec![7; 131072].into(),
        });
        let mut row = Vec::new();
        row.push(profile("overwrite-128k", scale, &|| {
            service.applied(Operation::Write {
                serial: file,
                position: layerfs_workspace::Position::At(0),
                data: vec![8; 131072].into(),
            });
        }));
        row.push(profile("shrink", scale, &|| {
            service.applied(Operation::SetAttributes {
                serial: file,
                mode: None,
                mtime: Some(T2),
                size: Some(65539),
            });
        }));
        row.push(profile("regrow", scale, &|| {
            service.applied(Operation::SetAttributes {
                serial: file,
                mode: None,
                mtime: Some(T1),
                size: Some(131072),
            });
        }));
        row.push(profile("read-128k", scale, &|| {
            let mut bytes = Vec::new();
            service
                .window(|view| view.read(&service.client, file, 0, 131072, &mut bytes))
                .unwrap();
            assert_eq!(bytes.len(), 131072);
            assert!(bytes[..65539].iter().all(|byte| *byte == 8));
            assert!(bytes[65539..].iter().all(|byte| *byte == 0));
        }));
        row.push(profile("append", scale, &|| {
            service.applied(Operation::Write {
                serial: file,
                position: layerfs_workspace::Position::End,
                data: b"tail".as_slice().into(),
            });
        }));
        if let Some(ref expected) = reference {
            assert_eq!(&row, expected);
        } else {
            reference = Some(row);
        }
    }
}
