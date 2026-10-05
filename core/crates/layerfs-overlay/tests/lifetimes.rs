mod payload_support;
use layerfs_overlay::*;
use payload_support::*;

fn drain(db: &Overlay) -> (u64, u64) {
    let mut cursor = MaintenanceCursor::default();
    let (mut jobs, mut bytes) = (0, 0);
    loop {
        assert!(jobs < 10000, "maintenance did not terminate");
        let Some(step) = db.maintain(cursor).unwrap() else {
            break;
        };
        assert!(step.work.rows <= 64 && step.work.data_bytes <= 65536);
        cursor = step.cursor;
        jobs += 1;
        bytes += step.work.data_bytes;
    }
    (jobs, bytes)
}

#[test]
fn definite_failures_compose_in_bounded_turns_without_growing_live_layers() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([101; 32], [102; 32]).unwrap();
    let mut source = db.acquire_base_source(route, 1).unwrap();
    let mut file = File::new(&db, source, 17, pattern(300001, 9));
    let window = pattern(131072, 3);
    file.write(0, &window);
    for round in 0..24 {
        db.release_base_source(source).unwrap();
        let capture = db.capture(route).unwrap();
        source = db.acquire_base_source(route, 100 + round).unwrap();
        file.source = source;
        file.resize(file.expect.len() as u64 / 2 + 5);
        file.write(8191, b"new bytes across a boundary");
        file.write(file.expect.len() as u64 + 5000, b"tail");
        let before = db.diagnostics();
        db.resolve_failed_capture(capture).unwrap();
        let after = db.diagnostics();
        let statements: u64 = before
            .statements
            .iter()
            .zip(after.statements)
            .map(|(a, b)| b.executions - a.executions)
            .sum();
        assert!(
            statements < 12,
            "failure must not fold payload: {statements}"
        );
        assert!(!db.capture_ready(route).unwrap());
        assert!(matches!(
            db.capture(route),
            Err(OverlayError::Consolidating)
        ));
        let mut cursor = MaintenanceCursor::default();
        for turn in 0..1000 {
            file.check();
            if turn % 3 == 0 {
                file.write((turn * 431 % 9000) as u64, b"interleaved");
            }
            let Some(step) = db.maintain(cursor).unwrap() else {
                break;
            };
            cursor = step.cursor;
            assert!(step.work.rows <= 64 && step.work.data_bytes <= 65536);
            assert!(turn < 999, "composition stalled");
        }
        file.check();
        assert!(db.state(route).unwrap().consolidating.is_none());
        assert!(db.capture_ready(route).unwrap());
        let cost = work(&db, || file.check());
        println!("S6_FAILURE round={round} transition_statements={statements} read_statements={} read_vm={} live_namespace_layers<=2",cost.0,cost.1);
    }
}

#[test]
fn reader_custody_parks_composition_then_exact_release_enables_it() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([103; 32], [104; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut file = File::new(&db, source, 19, Vec::new());
    file.write(0, b"sealed");
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = Lease {
        kind: LeaseKind::Reader,
        owner: 800,
        resource: capture.generation.number() as u64,
    };
    db.acquire(route, reader).unwrap();
    let source = db.acquire_base_source(route, 2).unwrap();
    file.source = source;
    file.write(0, b"active");
    db.resolve_failed_capture(capture).unwrap();
    let step = db.maintain(MaintenanceCursor::default()).unwrap().unwrap();
    assert_eq!(step.work.rows, 0);
    assert!(db.maintain(step.cursor).unwrap().is_none());
    file.check();
    assert!(db.state(route).unwrap().consolidating.is_some());
    db.release(route, reader).unwrap();
    drain(&db);
    file.check();
    assert!(db.capture_ready(route).unwrap());
}

#[test]
fn stale_cleanup_rechecks_epochs_and_never_deletes_regrown_new_bytes() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([105; 32], [106; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut file = File::new(&db, source, 20, Vec::new());
    for at in (0..4_u64 << 20).step_by(131072) {
        file.write(at, &pattern(131072, 5));
    }
    let allocated = db.pages().unwrap();
    file.resize(7);
    file.resize(4 << 20);
    file.write(2 << 20, b"new survives old discard");
    let (jobs, bytes) = drain(&db);
    file.check();
    let after = db.pages().unwrap();
    assert!(after.1 > allocated.1);
    println!("S6_STALE jobs={jobs} bytes_removed={bytes} pages_before={allocated:?} pages_after={after:?}");
    let plans = db.explain_maintenance().unwrap();
    assert!(plans
        .iter()
        .all(|p| p.contains("SEARCH") && !p.contains("TEMP")));
    println!("S6_PLANS {plans:?}");
}
