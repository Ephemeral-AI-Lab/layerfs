//! Allocation evidence for completion ownership: the Rust heap actually live
//! for held, parked and unattempted jobs never exceeds the bytes credited for
//! them. This binary holds exactly one test, so no other body moves the count.
//! Requested sizes are counted; allocator rounding is the declared allowance.
use layerfs_daemon::{
    Command, Completion, Owner, OwnerClient, OwnerConfig, OwnerError, Pending, Response,
};
use layerfs_overlay::{Inode, InodeKind, ProfileConfig, Route};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicIsize, Ordering},
    time::{Duration, Instant},
};

struct Counting;
static LIVE: AtomicIsize = AtomicIsize::new(0);
// SAFETY: every call forwards its unchanged arguments to the system allocator;
// the counter only observes the layouts of successful requests.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc(layout);
        if !pointer.is_null() {
            LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        System.dealloc(pointer, layout);
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

/// Live bytes with the owner idle. Transient owner-turn allocations only add,
/// so the minimum over a window is the retained amount.
fn live() -> isize {
    let mut least = isize::MAX;
    for _ in 0..400 {
        least = least.min(LIVE.load(Ordering::Relaxed));
        std::thread::yield_now();
    }
    least
}
fn submit(client: &OwnerClient, route: Option<Route>, command: Command) -> Pending {
    match client.try_submit(route, command) {
        Ok(pending) => pending,
        Err((error, command)) => panic!("admission of {command:?}: {error:?}"),
    }
}
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
fn settled(client: &OwnerClient, check: impl Fn(&layerfs_daemon::OwnerWork) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !check(&client.diagnostics().unwrap()) {
        assert!(Instant::now() < deadline, "owner state deadline");
        std::thread::yield_now();
    }
}
fn publish(serial: u64) -> Command {
    Command::Publish {
        inode: Inode {
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
        },
        name: None,
        cell: None,
    }
}
fn report(stage: &str, jobs: usize, measured: isize, credited: usize) {
    println!(
        "S7_COMPLETION_STORAGE stage={stage} jobs={jobs} live_heap_bytes={measured} \
         credited_bytes={credited} live_per_job={} credited_per_job={}",
        measured / jobs as isize,
        credited / jobs
    );
    assert!(measured > 0, "{stage}: no owned storage observed");
    assert!(
        measured as usize <= credited,
        "{stage}: live {measured} exceeds credit {credited}"
    );
}

#[test]
fn live_heap_of_held_parked_and_unattempted_jobs_stays_within_their_credit() {
    let path = std::env::temp_dir().join(format!("layerfs-storage-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let config = OwnerConfig::default();
    let owner = Owner::start(
        &path.join("overlay.sqlite"),
        ProfileConfig::default(),
        config,
    )
    .unwrap();
    let client = owner.client();
    let routes: Vec<Route> = (0..config.namespaces as u8)
        .map(|tag| {
            let done = finish(&submit(
                &client,
                None,
                Command::Open {
                    incarnation: [tag + 1; 32],
                    base_root: [77; 32],
                },
            ));
            match done.result() {
                Ok(Response::Opened(route)) => *route,
                other => panic!("open: {other:?}"),
            }
        })
        .collect();
    // Warm the connection's prepared statements and each lane before sampling.
    for route in &routes {
        assert!(finish(&submit(&client, Some(*route), Command::State))
            .result()
            .is_ok());
        assert!(finish(&submit(&client, Some(*route), publish(2)))
            .result()
            .is_ok());
    }
    settled(&client, |work| work.outstanding == 0);

    // Held: every lifecycle slot retains its result and receipt at once.
    let held: Vec<Completion> = routes
        .iter()
        .flat_map(|route| [*route; 2])
        .map(|route| finish(&submit(&client, Some(route), Command::State)))
        .collect();
    assert_eq!(
        held.len(),
        config.namespaces * config.lifecycle_jobs_per_namespace
    );
    let credited = client.diagnostics().unwrap().credited_bytes;
    let with = live();
    let jobs = held.len();
    drop(held);
    settled(&client, |work| work.outstanding == 0);
    report("held-lifecycle", jobs, with - live(), credited);

    // Parked: one capture per workspace waits behind its unsettled reply
    // ticket, keeping its queued job and its readiness receipt rows.
    let before = client
        .diagnostics()
        .unwrap()
        .sql_foreground
        .total()
        .attempts;
    let probe = submit(&client, Some(routes[0]), Command::Capture);
    settled(&client, |work| {
        work.queued == 1 && work.sql_foreground.total().attempts > before
    });
    let turn = client
        .diagnostics()
        .unwrap()
        .sql_foreground
        .total()
        .attempts
        - before;
    let without = live();
    let one = client.diagnostics().unwrap().credited_bytes;
    let mut parked = vec![probe];
    parked.extend(
        routes[1..]
            .iter()
            .map(|route| submit(&client, Some(*route), Command::Capture)),
    );
    let all = before + turn * routes.len() as u64;
    settled(&client, |work| {
        work.queued == routes.len() && work.sql_foreground.total().attempts == all
    });
    let credited = client.diagnostics().unwrap().credited_bytes;
    report(
        "parked-capture",
        routes.len() - 1,
        live() - without,
        credited - one,
    );

    // Unattempted: stop returns each original command with its receipt. The
    // owner thread is gone, so the drop delta is exactly their storage.
    owner.stop().unwrap();
    let stopped: Vec<Completion> = parked.iter().map(finish).collect();
    for done in &stopped {
        assert!(matches!(
            done.result(),
            Err(OwnerError::Unattempted { command, .. })
                if matches!(command.as_ref(), Command::Capture)
        ));
        assert!(done.work().parked_turns >= 1);
    }
    let credited = client.diagnostics().unwrap().credited_bytes;
    let with = live();
    let jobs = stopped.len();
    drop(stopped);
    drop(parked);
    assert_eq!(client.diagnostics().unwrap().outstanding, 0);
    report("unattempted-stop", jobs, with - live(), credited);
    std::fs::remove_dir_all(path).unwrap();
}
