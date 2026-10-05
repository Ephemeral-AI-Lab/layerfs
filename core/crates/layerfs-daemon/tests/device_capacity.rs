//! Real owner plus automatic idle reclamation on an exclusively owned ext4 fixture.
#![cfg(target_os = "linux")]
use layerfs_daemon::{Command, Completion, Owner, OwnerClient, OwnerConfig, OwnerError, Response};
use layerfs_overlay::{
    Cell, Inode, InodeKind, Lease, LeaseKind, OverlayError, ProfileConfig, Route, CLEANUP_HEADROOM,
};
use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};
fn run(client: &OwnerClient, route: Option<Route>, command: Command) -> Completion {
    let pending = client.try_submit(route, command).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "owner job deadline");
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        std::thread::yield_now();
    }
}
fn resources(client: &OwnerClient, route: Route) -> layerfs_overlay::Resources {
    match run(client, Some(route), Command::Resources { global: true }).result() {
        Ok(Response::Resources(r)) => **r,
        x => panic!("{x:?}"),
    }
}
#[test]
#[ignore = "requires exclusively owned ext4 loop fixture; device_fixture.sh daemon"]
fn real_owner_reclaims_at_device_full_after_last_owner_without_a_cleanup_job() {
    let root = PathBuf::from(std::env::var_os("LAYERFS_PROOF_DEVICE_ROOT").expect("owned fixture"));
    let owner = Owner::start(
        &root.join("owner.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let open = |tag| match run(
        &client,
        None,
        Command::Open {
            incarnation: [tag; 32],
            base_root: [17; 32],
        },
    )
    .result()
    {
        Ok(Response::Opened(r)) => *r,
        x => panic!("{x:?}"),
    };
    let route = open(191);
    let other = open(192);
    let mut inode = Inode {
        serial: 1,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: 1,
        mtime_nanoseconds: 2,
        nlink: 1,
        size: 4096,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
    };
    for serial in 1..=128 {
        inode.serial = serial;
        let done = run(
            &client,
            Some(route),
            Command::Publish {
                inode: inode.clone(),
                name: None,
                cell: Some(Cell {
                    offset: 0,
                    data: Box::new([91; 4096]),
                    validity: Box::new([255; 512]),
                }),
            },
        );
        let p = match done.result() {
            Ok(Response::Published(p)) => *p,
            x => panic!("{x:?}"),
        };
        drop(done);
        assert!(run(&client, Some(route), Command::ReplyAttempted(p))
            .result()
            .is_ok());
    }
    let capture = match run(&client, Some(route), Command::Capture).result() {
        Ok(Response::Captured(c)) => *c,
        x => panic!("{x:?}"),
    };
    let lease = Lease {
        kind: LeaseKind::Reader,
        owner: 901,
        resource: 0,
    };
    assert!(run(&client, Some(route), Command::Acquire(lease))
        .result()
        .is_ok());
    let other_state = match run(&client, Some(other), Command::State).result() {
        Ok(Response::State(s)) => s.clone(),
        x => panic!("{x:?}"),
    };
    // Filling the owned fixture is preparation, outside the product. The first
    // actual ENOSPC ends it; no failed allocation/write is replayed.
    let mut filler = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("owner-fixture-fill"))
        .unwrap();
    let bytes = vec![0; 1 << 20];
    let mut filled = 0;
    for _ in 0..512 {
        match filler.write_all(&bytes) {
            Ok(()) => filled += bytes.len(),
            Err(e) => {
                assert_eq!(e.raw_os_error(), Some(28));
                break;
            }
        }
    }
    let before = resources(&client, other);
    let mut accepted = 0;
    let mut reached_refusal = false;
    // Distinct new windows, not retries: reusable committed pages can admit a
    // bounded prefix even at device-full. Stop on the first actual refusal.
    for serial in 129..161 {
        inode.serial = serial;
        let work = client.diagnostics().unwrap().sql_foreground;
        let done = run(
            &client,
            Some(route),
            Command::Publish {
                inode: inode.clone(),
                name: None,
                cell: Some(Cell {
                    offset: 0,
                    data: Box::new([92; 4096]),
                    validity: Box::new([255; 512]),
                }),
            },
        );
        match done.result() {
            Ok(Response::Published(p)) => {
                let p = *p;
                drop(done);
                assert!(run(&client, Some(route), Command::ReplyAttempted(p))
                    .result()
                    .is_ok());
                accepted += 1;
            }
            Err(OwnerError::Overlay(OverlayError::Reservation { cause, .. })) if matches!(cause.as_ref(),OverlayError::Io(e) if e.raw_os_error()==Some(28)) =>
            {
                assert_eq!(
                    client.diagnostics().unwrap().sql_foreground.statements
                        [layerfs_overlay::StatementKind::Begin as usize]
                        .executions,
                    work.statements[layerfs_overlay::StatementKind::Begin as usize].executions
                );
                reached_refusal = true;
                break;
            }
            x => panic!("{x:?}"),
        }
    }
    assert!(reached_refusal, "new capacity did not refuse");
    assert_eq!(
        resources(&client, other).counts.payload_cells,
        before.counts.payload_cells + accepted
    );
    assert!(
        matches!(run(&client,Some(route),Command::RetainedCapture).result(),Ok(Response::RetainedCapture(Some(c))) if *c==capture)
    );
    assert!(run(&client, Some(route), Command::Close).result().is_ok());
    assert!(
        run(&client, Some(route), Command::ReleaseClosedCapture(capture))
            .result()
            .is_ok()
    );
    assert!(run(&client, Some(route), Command::Release(lease))
        .result()
        .is_ok());
    // Observe fixed in-memory service diagnostics only. No database status or
    // explicit cleanup job drives the idle worker.
    let deadline = Instant::now() + Duration::from_secs(5);
    while client.diagnostics().unwrap().maintenance_rows < (128 + accepted) * 2 + 2 {
        assert!(Instant::now() < deadline, "idle headroom cleanup stalled");
        assert!(client.maintenance_failure().unwrap().is_none());
        std::thread::yield_now();
    }
    let after = resources(&client, other);
    assert_eq!(after.counts.namespaces, 1);
    assert!(matches!(
        run(&client, Some(route), Command::CleanupState).result(),
        Ok(Response::CleanupState(layerfs_overlay::CleanupState::Gone))
    ));
    assert_eq!(after.counts.payload_cells, 0);
    assert!(after.free_pages > before.free_pages);
    assert!(after.allocation.reserved_tail_bytes >= CLEANUP_HEADROOM);
    assert!(
        matches!(run(&client,Some(other),Command::State).result(),Ok(Response::State(s)) if *s==other_state)
    );
    println!("S6_OWNER_DEVICE fixture_filled={filled} accepted_distinct_windows={accepted} before={before:?} after={after:?} owner_work={:?} automatic_idle=true ordinary_refusal=ENOSPC",client.diagnostics().unwrap());
    assert!(client.maintenance_failure().unwrap().is_none());
    owner.stop().unwrap();
}
