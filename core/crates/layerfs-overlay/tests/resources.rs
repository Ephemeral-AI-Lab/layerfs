mod payload_support;
use layerfs_overlay::*;
use payload_support::*;
#[test]
fn physical_reserve_counts_allocation_separately_from_logical_rows_and_reusable_pages() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([151; 32], [152; 32]).unwrap();
    assert_eq!(db.profile().auto_vacuum, 0);
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut f = File::new(&db, source, 7, Vec::new());
    f.write(0, &pattern(131072, 3));
    let r = db.resources(Some(route)).unwrap();
    assert_eq!(r.counts.payload_cells, 32);
    assert_eq!(r.counts.payload_bytes, 131072);
    assert!(r.allocation.allocated_bytes > r.allocation.logical_bytes + CLEANUP_HEADROOM);
    assert_eq!(r.counts.reply_tickets, 0);
    assert_eq!(r.counts.source_rows, 1);
    let global = db.resources(None).unwrap();
    assert_eq!(global.counts, r.counts);
    f.resize(1);
    let pending = db.resources(Some(route)).unwrap();
    assert!(pending.counts.maintenance_targets > 0 && pending.debt_upper_bytes > 0);
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..1000 {
        let Some(step) = db.maintain(cursor).unwrap() else {
            break;
        };
        cursor = step.cursor;
    }
    let cleaned = db.resources(Some(route)).unwrap();
    assert_eq!(cleaned.counts.payload_cells, 1);
    assert_eq!(cleaned.counts.payload_bytes, 1);
    assert_eq!(cleaned.counts.maintenance_targets, 0);
    assert!(cleaned.free_pages > r.free_pages);
    println!("S6_RESOURCES before={r:?} pending={pending:?} after={cleaned:?}");
    db.release_base_source(source).unwrap();
    db.close(route).unwrap();
    for _ in 0..1000 {
        if db.reclaim_closed(0).unwrap().unwrap().done {
            break;
        }
    }
    assert_eq!(db.resources(None).unwrap().counts, StoredCounts::default());
    let plan = db.explain_accounting().unwrap();
    assert!(plan
        .iter()
        .all(|p| p.contains("SEARCH") && !p.contains("TEMP")));
    println!("S6_ACCOUNTING_PLAN {plan:?}");
}

#[test]
fn retained_snapshot_parks_orphan_migration_then_rows_move_without_payload_duplication() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([181; 32], [182; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut f = File::new(&db, source, 7, Vec::new());
    for at in (0..2 << 20).step_by(131072) {
        f.write(at, &pattern(131072, 3));
    }
    let open = db.open_file(source, 1, &f.inode(2 << 20), true).unwrap();
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    let source = db.acquire_base_source(route, 2).unwrap();
    let mut inode = f.inode(2 << 20);
    inode.nlink = 0;
    let p = db
        .apply(
            source,
            &Changes {
                inodes: vec![inode],
                ..Changes::default()
            },
        )
        .unwrap();
    db.reply_attempted(p).unwrap();
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..100 {
        let Some(step) = db.maintain(cursor).unwrap() else {
            break;
        };
        cursor = step.cursor;
    }
    let held = db.resources(Some(route)).unwrap();
    assert_eq!(held.counts.payload_cells, 512);
    assert_eq!(held.counts.payload_bytes, 2 << 20);
    assert_eq!(held.counts.wait_refs, 1);
    db.resolve_failed_capture(capture).unwrap();
    db.release_captured_reader(reader).unwrap();
    for turn in 0..2000 {
        let Some(step) = db.maintain(cursor).unwrap() else {
            break;
        };
        cursor = step.cursor;
        assert!(
            db.resources(Some(route)).unwrap().counts.payload_cells <= 512,
            "payload duplication at turn {turn}"
        );
        assert!(turn < 1999);
    }
    let moved = db.resources(Some(route)).unwrap();
    assert_eq!(moved.counts.payload_cells, 512);
    assert_eq!(moved.counts.payload_bytes, 2 << 20);
    assert_eq!(moved.counts.wait_refs, 0);
    let read = db.acquire_file_read(source, open, 1).unwrap();
    for at in (0..2 << 20).step_by(131072) {
        assert_eq!(
            db.read_file(read, at, 131072).unwrap().unwrap().data,
            pattern(131072, 3)
        );
    }
    db.release_file_read(read).unwrap();
    db.close_file(open).unwrap();
    for _ in 0..1000 {
        let Some(step) = db.maintain(cursor).unwrap() else {
            break;
        };
        cursor = step.cursor;
    }
    assert_eq!(db.resources(Some(route)).unwrap().counts.payload_cells, 0);
    println!("S6_ORPHAN_MOVE held={held:?} moved={moved:?} maximum_payload_cells=512 snapshot_wait_refs=1->0 original_bytes_preserved=true");
}

#[test]
fn last_generation_release_is_fixed_work_and_wakes_retirements_in_bounded_pages() {
    let mut release_work = Vec::new();
    for population in [128, 1024, 4096] {
        let temp = Temp::new();
        let db = temp.db();
        let route = db.open_workspace([201; 32], [202; 32]).unwrap();
        let source = db.acquire_base_source(route, 1).unwrap();
        // Each absent unlinked serial has one targeted retirement item.
        for serial in 1..=population {
            let inode = File::new(&db, source, serial, Vec::new()).inode(0);
            let mut removed = inode;
            removed.nlink = 0;
            let p = db
                .apply(
                    source,
                    &Changes {
                        inodes: vec![removed],
                        ..Changes::default()
                    },
                )
                .unwrap();
            db.reply_attempted(p).unwrap();
        }
        db.release_base_source(source).unwrap();
        let capture = db.capture(route).unwrap();
        let lease = Lease {
            kind: LeaseKind::Reader,
            owner: 901,
            resource: capture.generation.number() as u64,
        };
        db.acquire(route, lease).unwrap();
        let mut cursor = MaintenanceCursor::default();
        for turn in 0..population + 10 {
            let Some(step) = db.maintain(cursor).unwrap() else {
                break;
            };
            cursor = step.cursor;
            assert!(turn < population + 9);
        }
        assert_eq!(db.resources(Some(route)).unwrap().counts.ready_targets, 0);
        release_work.push(work(&db, || db.release(route, lease).unwrap()));
        let first = work(&db, || {
            let step = db.maintain(cursor).unwrap().unwrap();
            cursor = step.cursor;
            assert_eq!(step.work.rows, 64);
        });
        let mut turns = 1;
        while let Some(step) = db.maintain(cursor).unwrap() {
            cursor = step.cursor;
            assert!(step.work.rows <= 64);
            turns += 1;
            assert!(turns < population * 10);
        }
        assert_eq!(
            db.resources(Some(route))
                .unwrap()
                .counts
                .maintenance_targets,
            0
        );
        println!("S6_GENERATION_WAKE population={population} release={:?} first_page={first:?} bounded_turns={turns} plans={:?}",release_work.last().unwrap(),db.explain_maintenance().unwrap());
    }
    assert!(
        release_work.windows(2).all(|p| p[0] == p[1]),
        "release grew with target population: {release_work:?}"
    );
}

#[test]
fn lost_backing_path_quarantines_without_sql_or_releasing_published_capture() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([211; 32], [212; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut f = File::new(&db, source, 7, Vec::new());
    f.write(0, b"retained publication");
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let source = db.acquire_base_source(route, 2).unwrap();
    let prior = db.diagnostics();
    let path = temp.0.join("overlay.sqlite");
    let bytes = std::fs::read(&path).unwrap();
    let retained = temp.0.join("retained.sqlite");
    std::fs::rename(&path, &retained).unwrap();
    let error = db
        .apply(
            source,
            &Changes {
                inodes: vec![f.inode(20)],
                ..Changes::default()
            },
        )
        .unwrap_err();
    assert!(
        matches!(&error,OverlayError::Uncertain {cause,completion:None} if matches!(cause.as_ref(),OverlayError::Io(e) if e.kind()==std::io::ErrorKind::NotFound)),
        "{error:?}"
    );
    assert_eq!(
        db.diagnostics().statements[StatementKind::Begin as usize].executions,
        prior.statements[StatementKind::Begin as usize].executions
    );
    assert_eq!(std::fs::read(&retained).unwrap(), bytes);
    assert!(matches!(
        db.resolve_failed_capture(capture),
        Err(OverlayError::Quarantined)
    ));
    assert!(matches!(
        db.release_base_source(source),
        Err(OverlayError::Quarantined)
    ));
    println!("S6_UNCERTAIN_PATH error={error:?} backing_unchanged=true begin_not_attempted=true exact_capture={capture:?} release_refused=true");
}

#[test]
fn real_backing_corruption_retains_original_sql_cause_and_quarantines_custody() {
    let temp = Temp::new();
    let path = temp.0.join("overlay.sqlite");
    let db = Overlay::create(
        &path,
        ProfileConfig {
            pager_kib: 4,
            max_pages: None,
        },
    )
    .unwrap();
    let route = db.open_workspace([213; 32], [214; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut f = File::new(&db, source, 7, Vec::new());
    for at in (0..1 << 20).step_by(131072) {
        f.write(at, &pattern(131072, 3));
    }
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    // External corruption of this test's own backing exercises SQLite's actual
    // unsafe read result. It is not an allocation/full-disk durability claim.
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(0)
        .unwrap();
    let error = db.captured_read(capture, 7, 0, 131072).unwrap_err();
    assert!(
        matches!(&error,OverlayError::Uncertain {cause,..} if matches!(cause.as_ref(),OverlayError::Sql(_))),
        "{error:?}"
    );
    assert!(path.exists());
    assert!(matches!(
        db.release_closed_capture(capture),
        Err(OverlayError::Quarantined)
    ));
    assert!(matches!(db.close(route), Err(OverlayError::Quarantined)));
    println!("S6_UNCERTAIN_SQL error={error:?} exact_capture={capture:?} artifact_retained=true no_release_authority=true");
}

#[test]
fn repeated_resource_snapshots_do_not_reprepare_expired_metadata_statements() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([215; 32], [216; 32]).unwrap();
    let first = db.resources(Some(route)).unwrap();
    let before = db.diagnostics();
    for _ in 0..8 {
        assert_eq!(db.resources(Some(route)).unwrap().counts, first.counts);
    }
    let after = db.diagnostics();
    assert!(before
        .statements
        .iter()
        .zip(after.statements)
        .all(|(a, b)| a.reprepares == b.reprepares));
    println!(
        "S6_RESOURCE_SNAPSHOTS count=8 reprepare=0 indexed_counts=true physical_page_readback=true"
    );
}
