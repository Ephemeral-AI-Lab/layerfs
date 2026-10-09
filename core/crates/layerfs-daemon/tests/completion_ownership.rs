//! Original per-job statement families, their owner aggregate and the credited
//! completion ownership behind them, through the public owner API only.
use layerfs_daemon::{
    Command, Completion, Owner, OwnerClient, OwnerConfig, OwnerError, OwnerWork, Pending, Response,
};
use layerfs_overlay::{
    AllocationWork, DatabaseWork, Inode, InodeKind, Lease, LeaseKind, OperationRecord,
    OverlayError, PayloadWork, ProfileConfig, Publication, Route, StatementKind,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

/// Families a Lifecycle job is admitted for; its slot charge reserves these.
const LIFECYCLE_FAMILIES: usize = 7;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-completion-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn start(temp: &Temp) -> Owner {
    Owner::start(
        &temp.0.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap()
}
fn submit(client: &OwnerClient, route: Option<Route>, command: Command) -> Pending {
    match client.try_submit(route, command) {
        Ok(pending) => pending,
        Err((error, command)) => panic!("admission of {command:?}: {error:?}"),
    }
}
/// Bounded wait: a stalled owner fails the test instead of hanging it.
fn finish(pending: &Pending) -> Completion {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < deadline, "owner completion deadline");
        std::thread::yield_now();
    }
}
/// Publication makes the result visible before the publisher drops its own
/// original cell. Observe final release instead of assuming the caller wins
/// after that drop. This issues no new job and retains a bounded deadline.
fn credit_count(client: &OwnerClient, expected: usize) -> OwnerWork {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let work = client.diagnostics().unwrap();
        if work.outstanding == expected {
            return work;
        }
        assert!(
            Instant::now() < deadline,
            "credit release deadline: {work:?}"
        );
        std::thread::yield_now();
    }
}
/// Waits until the one job submitted after `before` has run a readiness turn
/// and is queued again: its turn is published before it is requeued.
fn parked(client: &OwnerClient, before: &OwnerWork) {
    let attempts = before.sql_foreground.total().attempts;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let now = client.diagnostics().unwrap();
        if now.queued == 1 && now.sql_foreground.total().attempts > attempts {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "job never parked: queued={} outstanding={} attempts={} before={attempts}",
            now.queued,
            now.outstanding,
            now.sql_foreground.total().attempts
        );
        std::thread::yield_now();
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
        born: 0,
        entries: 0,
    }
}

/// Sums original receipts and compares them with the same owner's foreground
/// aggregate. Maintenance has its own aggregate and never enters either side.
struct Ledger {
    client: OwnerClient,
    before: OwnerWork,
    sql: DatabaseWork,
    payload: PayloadWork,
    allocation: AllocationWork,
    lifecycle_max: usize,
    parked: u64,
}
impl Ledger {
    fn new(client: &OwnerClient) -> Self {
        Self {
            client: client.clone(),
            before: client.diagnostics().unwrap(),
            sql: DatabaseWork::default(),
            payload: PayloadWork::default(),
            allocation: AllocationWork::default(),
            lifecycle_max: 0,
            parked: 0,
        }
    }
    fn take(&mut self, name: &str, lifecycle: bool, done: &Completion) {
        let work = done.work();
        assert_eq!(work.sql.total(), work.sql.expanded().total());
        assert_eq!(
            work.sql.families(),
            work.sql
                .expanded()
                .statements
                .iter()
                .filter(|row| row.attempts != 0)
                .count(),
            "{name}: a family row exists exactly for attempted statements"
        );
        self.sql.accumulate(work.sql.expanded());
        self.payload.accumulate(work.payload);
        self.allocation.accumulate(work.allocation);
        self.parked += work.parked_turns;
        if lifecycle {
            self.lifecycle_max = self.lifecycle_max.max(work.sql.families());
        }
        println!(
            "S7_JOB_FAMILIES command={name} lifecycle={lifecycle} families={} parked_turns={} ok={}",
            work.sql.families(),
            work.parked_turns,
            done.result().is_ok()
        );
    }
    /// Valid whenever no admitted job is still unfinished: every visible
    /// completion's work is already in the owner aggregate, and nothing else is.
    fn reconcile(&self) {
        let now = self.client.diagnostics().unwrap();
        assert_eq!(
            now.sql_foreground.since(&self.before.sql_foreground),
            self.sql
        );
        assert_eq!(
            now.payload_foreground.since(self.before.payload_foreground),
            self.payload
        );
        assert_eq!(
            now.allocation_foreground
                .since(self.before.allocation_foreground),
            self.allocation
        );
    }
    fn run(&mut self, name: &str, lifecycle: bool, route: Route, command: Command) -> Completion {
        let done = finish(&submit(&self.client, Some(route), command));
        self.take(name, lifecycle, &done);
        self.reconcile();
        done
    }
}

#[test]
fn original_family_receipts_equal_the_foreground_aggregate_on_every_path() {
    let temp = Temp::new();
    let owner = start(&temp);
    let client = owner.client();
    let mut ledger = Ledger::new(&client);
    let opened = finish(&submit(
        &client,
        None,
        Command::Open {
            incarnation: [31; 32],
            base_root: [32; 32],
        },
    ));
    ledger.take("Open", true, &opened);
    ledger.reconcile();
    let route = match opened.result() {
        Ok(Response::Opened(route)) => *route,
        other => panic!("open: {other:?}"),
    };
    let open_work = opened.work();
    for kind in [StatementKind::Begin, StatementKind::Commit] {
        assert_eq!(open_work.sql.family(kind).unwrap().attempts, 1);
    }
    assert!(open_work.sql.family(StatementKind::Rollback).is_none());
    assert!(open_work.sql.family(StatementKind::Payload).is_none());
    drop(opened);

    for (name, command) in [
        ("State", Command::State),
        ("CleanupState", Command::CleanupState),
        ("MaintenanceIdle", Command::MaintenanceIdle),
        ("RetainedCapture", Command::RetainedCapture),
        (
            "RetainedBaseSource",
            Command::RetainedBaseSource { owner: 5 },
        ),
        ("RetainedFile", Command::RetainedFile { request: 1 }),
        ("RetainedFileRead", Command::RetainedFileRead { request: 1 }),
        (
            "RetainedCapturedReader",
            Command::RetainedCapturedReader { request: 1 },
        ),
        ("RetainedLookup", Command::RetainedLookup { request: 1 }),
        (
            "RetainedOperation",
            Command::RetainedOperation { request: 1 },
        ),
    ] {
        assert!(ledger.run(name, true, route, command).result().is_ok());
    }
    let lease = Lease {
        kind: LeaseKind::Reader,
        owner: 7,
        resource: 0,
    };
    assert!(ledger
        .run("Acquire", true, route, Command::Acquire(lease))
        .result()
        .is_ok());
    assert!(ledger
        .run("Release", true, route, Command::Release(lease))
        .result()
        .is_ok());
    let operation = match ledger
        .run(
            "AcquireOperation",
            true,
            route,
            Command::AcquireOperation { request: 1 },
        )
        .result()
    {
        Ok(Response::Operation(Some(owner))) => *owner,
        other => panic!("operation: {other:?}"),
    };
    assert!(ledger
        .run(
            "ReleaseOperation",
            true,
            route,
            Command::ReleaseOperation(operation)
        )
        .result()
        .is_ok());

    // A reply attempt on the owner's turn, then the same attempt again. The
    // ticket is held in the engine's memory, so neither job writes: the
    // first finds a live Workspace with nothing to queue, and the second
    // fails on a ticket that is no longer held, after its route's read.
    let publish = |ledger: &mut Ledger, serial: u64| -> Publication {
        let done = ledger.run(
            "Publish",
            false,
            route,
            Command::Publish {
                inode: value(serial),
                name: None,
                cell: None,
            },
        );
        match done.result() {
            Ok(Response::Published(publication)) => *publication,
            other => panic!("publish: {other:?}"),
        }
    };
    let first = publish(&mut ledger, 2);
    let replied = ledger.run(
        "ReplyAttempted",
        true,
        route,
        Command::ReplyAttempted(first),
    );
    assert!(replied.result().is_ok());
    let failed = ledger.run(
        "ReplyAttempted-again",
        true,
        route,
        Command::ReplyAttempted(first),
    );
    assert!(matches!(
        failed.result(),
        Err(OwnerError::Overlay(OverlayError::Stale))
    ));
    for done in [&replied, &failed] {
        for kind in [
            StatementKind::Begin,
            StatementKind::Commit,
            StatementKind::Rollback,
            StatementKind::Frontier,
        ] {
            assert!(done.work().sql.family(kind).is_none(), "{kind:?}");
        }
        assert_eq!(done.work().sql.total().rows_changed, 0);
        // What each did run is in its own receipt: the route's point read.
        assert!(done.work().sql.family(StatementKind::Workspace).is_some());
    }
    drop((replied, failed));

    let source = match ledger
        .run(
            "AcquireBaseSource",
            false,
            route,
            Command::AcquireBaseSource { owner: 7 },
        )
        .result()
    {
        Ok(Response::BaseSource(source)) => *source,
        other => panic!("source: {other:?}"),
    };
    // A job that fails inside its transaction and rolls back keeps its own
    // actual families: the same owner's second row is refused by its insert.
    let failed = ledger.run(
        "AcquireBaseSource-again",
        false,
        route,
        Command::AcquireBaseSource { owner: 7 },
    );
    assert!(matches!(failed.result(), Err(OwnerError::Overlay(_))));
    assert_eq!(
        failed
            .work()
            .sql
            .family(StatementKind::Rollback)
            .unwrap()
            .attempts,
        1
    );
    assert_eq!(
        failed
            .work()
            .sql
            .family(StatementKind::Begin)
            .unwrap()
            .attempts,
        1
    );
    assert!(failed.work().sql.family(StatementKind::Commit).is_none());
    drop(failed);
    let file = match ledger
        .run(
            "OpenFile",
            true,
            route,
            Command::OpenFile {
                source,
                request: 11,
                base: value(2),
                writable: true,
            },
        )
        .result()
    {
        Ok(Response::File(Some(file))) => *file,
        other => panic!("file: {other:?}"),
    };
    let read = match ledger
        .run(
            "AcquireFileRead",
            true,
            route,
            Command::AcquireFileRead {
                source,
                file,
                request: 12,
            },
        )
        .result()
    {
        Ok(Response::FileReader(Some(read))) => *read,
        other => panic!("read: {other:?}"),
    };
    let lookup = match ledger
        .run(
            "AcquireLookup",
            true,
            route,
            Command::AcquireLookup {
                source,
                request: 13,
                base: value(2),
                root: 1,
            },
        )
        .result()
    {
        Ok(Response::Lookup(Some(lookup))) => *lookup,
        other => panic!("lookup: {other:?}"),
    };
    let lookup_read = match ledger
        .run(
            "AcquireLookupRead",
            true,
            route,
            Command::AcquireLookupRead {
                source,
                lookup,
                request: 14,
            },
        )
        .result()
    {
        Ok(Response::FileReader(Some(read))) => *read,
        other => panic!("lookup read: {other:?}"),
    };
    for (name, command) in [
        ("ReleaseFileRead", Command::ReleaseFileRead(lookup_read)),
        ("ReleaseLookup", Command::ReleaseLookup(lookup)),
        ("ReleaseFileRead", Command::ReleaseFileRead(read)),
        ("CloseFile", Command::CloseFile(file)),
        ("ReleaseBaseSource", Command::ReleaseBaseSource(source)),
    ] {
        let done = ledger.run(name, true, route, command);
        assert!(done.result().is_ok(), "{name}: {done:?}");
    }

    // A capture parks on the unsettled reply ticket. Its readiness turns are
    // foreground work before its completion exists, so reconcile afterwards.
    let pending_reply = finish(&submit(
        &client,
        Some(route),
        Command::Publish {
            inode: value(3),
            name: None,
            cell: None,
        },
    ));
    ledger.take("Publish", false, &pending_reply);
    let ticket = match pending_reply.result() {
        Ok(Response::Published(publication)) => *publication,
        other => panic!("publish: {other:?}"),
    };
    let capture = submit(&client, Some(route), Command::Capture);
    let state = finish(&submit(&client, Some(route), Command::State));
    ledger.take("State", true, &state);
    assert!(capture.try_complete().unwrap().is_none());
    let replied = finish(&submit(
        &client,
        Some(route),
        Command::ReplyAttempted(ticket),
    ));
    ledger.take("ReplyAttempted", true, &replied);
    let captured = finish(&capture);
    ledger.take("Capture", false, &captured);
    assert!(captured.work().parked_turns >= 1);
    ledger.reconcile();
    // Held lifecycle results occupy their namespace's two slots until dropped.
    drop((state, replied, pending_reply));
    let capture = match captured.result() {
        Ok(Response::Captured(capture)) => *capture,
        other => panic!("capture: {other:?}"),
    };
    let reader = match ledger
        .run(
            "AcquireCapturedReader",
            true,
            route,
            Command::AcquireCapturedReader {
                capture,
                request: 21,
            },
        )
        .result()
    {
        Ok(Response::CapturedReader(Some(reader))) => *reader,
        other => panic!("reader: {other:?}"),
    };
    for (name, command) in [
        (
            "ReleaseCapturedReader",
            Command::ReleaseCapturedReader(reader),
        ),
        ("ResolveFailed", Command::ResolveFailed(capture)),
    ] {
        let done = ledger.run(name, true, route, command);
        assert!(done.result().is_ok(), "{name}: {done:?}");
    }

    // The next capture may park for automatic composition; close with it held.
    let last = publish(&mut ledger, 4);
    assert!(ledger
        .run("ReplyAttempted", true, route, Command::ReplyAttempted(last))
        .result()
        .is_ok());
    let next = finish(&submit(&client, Some(route), Command::Capture));
    ledger.take("Capture", false, &next);
    ledger.reconcile();
    let next = match next.result() {
        Ok(Response::Captured(capture)) => *capture,
        other => panic!("capture: {other:?}"),
    };
    for (name, command) in [
        ("Close", Command::Close),
        ("ReleaseClosedCapture", Command::ReleaseClosedCapture(next)),
        ("CleanupState", Command::CleanupState),
    ] {
        let done = ledger.run(name, true, route, command);
        assert!(done.result().is_ok(), "{name}: {done:?}");
    }

    let after = client.diagnostics().unwrap();
    assert!(ledger.parked >= 1);
    assert!(ledger.lifecycle_max <= LIFECYCLE_FAMILIES);
    assert_eq!(after.receipt_overruns, 0);
    assert_eq!(after.receipt_overrun_bytes, 0);
    assert_eq!(after.queued, 0);
    assert!(after.peak_queued >= 2);
    println!(
        "S7_FAMILY_LEDGER lifecycle_max_families={} parked_turns={} foreground_attempts={}",
        ledger.lifecycle_max,
        ledger.parked,
        ledger.sql.total().attempts
    );
    assert!(client.maintenance_failure().unwrap().is_none());
    owner.stop().unwrap();
}

#[test]
fn every_configured_lifecycle_slot_is_admitted_while_ordinary_credit_is_saturated() {
    let temp = Temp::new();
    let config = OwnerConfig::default();
    assert_eq!(
        (
            config.bytes,
            config.lifecycle_reserve,
            config.namespaces,
            config.lifecycle_jobs_per_namespace
        ),
        (8 * 1024 * 1024, 64 * 1024, 16, 2)
    );
    let owner = start(&temp);
    let client = owner.client();
    let routes: Vec<Route> = (0..config.namespaces as u8)
        .map(|tag| {
            let done = finish(&submit(
                &client,
                None,
                Command::Open {
                    incarnation: [tag + 1; 32],
                    base_root: [99; 32],
                },
            ));
            match done.result() {
                Ok(Response::Opened(route)) => *route,
                other => panic!("open: {other:?}"),
            }
        })
        .collect();
    let idle = credit_count(&client, 0);
    assert_eq!((idle.outstanding, idle.credited_bytes), (0, 0));
    let scheduler = idle.scheduler_bytes;
    assert!(scheduler > 0);

    // Actual charges, read from the ledger of one held result of each kind.
    let held = finish(&submit(&client, Some(routes[0]), Command::State));
    let lifecycle_charge = client.diagnostics().unwrap().credited_bytes;
    drop(held);
    assert_eq!(credit_count(&client, 0).credited_bytes, 0);
    let held = finish(&submit(&client, Some(routes[0]), Command::Inode(2)));
    let ordinary_charge = client.diagnostics().unwrap().credited_bytes;
    drop(held);
    assert_eq!(credit_count(&client, 0).credited_bytes, 0);
    let slots = config.namespaces * config.lifecycle_jobs_per_namespace;
    assert_eq!(slots, 32);
    assert!(slots * lifecycle_charge <= config.lifecycle_reserve);

    // Saturate ordinary bytes with held results owning real input capacity.
    let ordinary_limit = config.bytes - scheduler - config.lifecycle_reserve;
    let mut ordinary = Vec::new();
    let mut capacity = 60_000;
    let mut turn = 0;
    while capacity > 0 {
        let route = routes[turn % routes.len()];
        match client.try_submit(
            Some(route),
            Command::PutOperationRecord {
                operation: 1,
                record: OperationRecord {
                    kind: 1,
                    key: turn as u64,
                    value: Vec::with_capacity(capacity),
                },
            },
        ) {
            Ok(pending) => {
                ordinary.push(finish(&pending));
                turn += 1;
            }
            Err((OwnerError::AdmissionFull, _)) => capacity /= 2,
            Err((error, _)) => panic!("ordinary admission: {error:?}"),
        }
    }
    let saturated = client.diagnostics().unwrap();
    assert_eq!(saturated.outstanding, ordinary.len());
    assert!(saturated.credited_bytes <= ordinary_limit);
    assert!(ordinary_limit - saturated.credited_bytes < ordinary_charge);
    assert!(ordinary.len() < config.namespaces * config.jobs_per_namespace);
    let route = routes[(turn + 1) % routes.len()];
    assert!(matches!(
        client.try_submit(Some(route), Command::Inode(2)),
        Err((OwnerError::AdmissionFull, Command::Inode(2)))
    ));

    // All 32 lifecycle slots are admitted, served and held at once.
    let mut lifecycle = Vec::new();
    for route in &routes {
        for _ in 0..config.lifecycle_jobs_per_namespace {
            let done = finish(&submit(&client, Some(*route), Command::State));
            assert!(matches!(done.result(), Ok(Response::State(_))));
            assert!(done.work().sql.families() <= LIFECYCLE_FAMILIES);
            lifecycle.push(done);
        }
    }
    assert_eq!(lifecycle.len(), slots);
    let full = client.diagnostics().unwrap();
    assert_eq!(full.outstanding, ordinary.len() + slots);
    assert_eq!(
        full.credited_bytes,
        saturated.credited_bytes + slots * lifecycle_charge
    );
    assert!(full.peak_credited_bytes <= config.bytes - scheduler);
    assert_eq!(full.receipt_overruns, 0);
    // The slot count itself still refuses a third job in one namespace.
    assert!(matches!(
        client.try_submit(Some(routes[0]), Command::State),
        Err((OwnerError::AdmissionFull, Command::State))
    ));
    println!(
        "S7_COMPLETION_CREDIT bytes={} lifecycle_reserve={} scheduler_bytes={scheduler} \
         lifecycle_charge={lifecycle_charge} lifecycle_slots={slots} \
         lifecycle_slot_bytes={} ordinary_default_charge={ordinary_charge} \
         ordinary_held={} ordinary_credited={} peak_credited={}",
        config.bytes,
        config.lifecycle_reserve,
        slots * lifecycle_charge,
        ordinary.len(),
        saturated.credited_bytes,
        full.peak_credited_bytes
    );
    drop(lifecycle);
    drop(ordinary);
    let released = credit_count(&client, 0);
    assert_eq!((released.outstanding, released.credited_bytes), (0, 0));
    owner.stop().unwrap();
}

#[test]
fn a_completion_is_handed_out_once_and_wakes_its_blocked_waiter() {
    let temp = Temp::new();
    let owner = start(&temp);
    let client = owner.client();
    let opened = finish(&submit(
        &client,
        None,
        Command::Open {
            incarnation: [61; 32],
            base_root: [62; 32],
        },
    ));
    let route = match opened.result() {
        Ok(Response::Opened(route)) => *route,
        other => panic!("open: {other:?}"),
    };
    drop(opened);

    // One-shot delivery: the taken result stays valid, later takes disconnect,
    // and the credit lasts until both the Pending and the Completion are gone.
    let pending = submit(&client, Some(route), Command::State);
    let done = finish(&pending);
    assert!(matches!(
        pending.try_complete(),
        Err(OwnerError::Disconnected)
    ));
    assert!(matches!(done.result(), Ok(Response::State(_))));
    assert!(matches!(pending.wait(), Err(OwnerError::Disconnected)));
    assert_eq!(credit_count(&client, 1).outstanding, 1);
    drop(done);
    assert_eq!(credit_count(&client, 0).outstanding, 0);

    // A waiter blocked on a parked capture is woken by its publication. If this
    // thread fails first, dropping the owner returns the capture unattempted.
    let published = finish(&submit(
        &client,
        Some(route),
        Command::Publish {
            inode: value(2),
            name: None,
            cell: None,
        },
    ));
    let ticket = match published.result() {
        Ok(Response::Published(publication)) => *publication,
        other => panic!("publish: {other:?}"),
    };
    drop(published);
    let before = client.diagnostics().unwrap();
    let capture = submit(&client, Some(route), Command::Capture);
    let waiter = std::thread::spawn(move || capture.wait());
    parked(&client, &before);
    assert!(!waiter.is_finished());
    let replied = finish(&submit(
        &client,
        Some(route),
        Command::ReplyAttempted(ticket),
    ));
    assert!(matches!(replied.result(), Ok(Response::Done)));
    drop(replied);
    let captured = waiter.join().unwrap().unwrap();
    assert!(matches!(captured.result(), Ok(Response::Captured(_))));
    assert!(captured.work().parked_turns >= 1);

    // Receiver loss neither cancels an admitted job nor leaks its credit.
    drop(submit(&client, Some(route), Command::State));
    let deadline = Instant::now() + Duration::from_secs(20);
    while client.diagnostics().unwrap().outstanding != 1 {
        assert!(Instant::now() < deadline, "lost receiver kept its credit");
        std::thread::yield_now();
    }
    drop(captured);

    // Stop returns the exact unattempted command to a blocked waiter. The
    // first workspace still retains its capture, so this one parks elsewhere.
    let opened = finish(&submit(
        &client,
        None,
        Command::Open {
            incarnation: [63; 32],
            base_root: [62; 32],
        },
    ));
    let route = match opened.result() {
        Ok(Response::Opened(route)) => *route,
        other => panic!("open: {other:?}"),
    };
    drop(opened);
    let unsettled = finish(&submit(
        &client,
        Some(route),
        Command::Publish {
            inode: value(3),
            name: None,
            cell: None,
        },
    ));
    assert!(unsettled.result().is_ok());
    let before = client.diagnostics().unwrap();
    let capture = submit(&client, Some(route), Command::Capture);
    let waiter = std::thread::spawn(move || capture.wait());
    parked(&client, &before);
    owner.stop().unwrap();
    let stopped = waiter.join().unwrap().unwrap();
    assert!(matches!(
        stopped.result(),
        Err(OwnerError::Unattempted { cause, command })
            if matches!(cause.as_ref(), OwnerError::Stopped)
                && matches!(command.as_ref(), Command::Capture)
    ));
    // The parked readiness work it already did stays on the original receipt.
    assert!(stopped.work().parked_turns >= 1);
    assert!(stopped.work().sql.families() >= 1);
}
