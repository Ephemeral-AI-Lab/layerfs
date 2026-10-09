mod payload_support;
use layerfs_overlay::*;
use payload_support::*;

#[test]
fn complete_mutation_reports_triggers_blob_delivery_and_physical_reservation() {
    let temp = Temp::new();
    let db = temp.db();
    let initial = db.allocation().unwrap();
    assert!(initial.allocated_bytes >= MUTATION_GROWTH + CLEANUP_HEADROOM);
    let route = db.open_workspace([231; 32], [232; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut file = File::new(&db, source, 7, Vec::new());
    let sql = db.diagnostics();
    let physical = db.allocation_work();
    let payload = db.payload_work();
    file.write(0, &pattern(CELL_BYTES, 1));
    let mutation = db.diagnostics().since(&sql).total();
    let admission = db.allocation_work().since(physical);
    let copies = db.payload_work().since(payload);
    assert_eq!(copies.write_cells, 1);
    assert_eq!(copies.partial_write_cells, 0);
    assert_eq!(copies.write_input_bytes, CELL_BYTES as u64);
    // A whole cell is bound from the caller's slice: the engine copies none.
    assert_eq!(copies.cell_copy_bytes, 0);
    assert_eq!(copies.cell_zeroed_bytes, 0);
    assert_eq!(copies.in_place_writes, 0);
    // File::write includes the mutation and its reply attempt. Only the
    // mutation writes: the attempt returns a ticket held in the engine's
    // memory, and its owner turn finds a live Workspace with nothing to
    // queue, so it begins no transaction and pays no admission.
    // Count-trigger changes of the mutation remain visible.
    assert_eq!(admission.freelist_queries, 1);
    assert_eq!(admission.admitted_jobs, 1);
    assert_eq!(admission.refusals, 0);
    // The file has not grown since its range was last established, so
    // the admission does not allocate, on Linux as on macOS.
    assert_eq!(admission.attempts, 0);
    assert_eq!(admission.observations, 1);
    let work = db.diagnostics().since(&sql);
    assert_eq!(work.statements[StatementKind::Begin as usize].executions, 1);
    assert_eq!(
        work.statements[StatementKind::Commit as usize].executions,
        1
    );
    assert_eq!(
        work.statements[StatementKind::Rollback as usize].executions,
        0
    );
    assert!(mutation.rows_changed > mutation.direct_rows_changed);
    assert_eq!(
        mutation.fullscan_steps + mutation.sorts + mutation.reprepares,
        0
    );
    let before = db.diagnostics();
    assert_eq!(db.inode(route, 7).unwrap().unwrap().size, CELL_BYTES as u64);
    assert_eq!(
        db.diagnostics().since(&before).total().returned_blob_bytes,
        32
    );
    let before = db.diagnostics();
    assert_eq!(file.read(0, CELL_BYTES as u32), file.expect);
    let read = db.diagnostics().since(&before).total();
    assert_eq!(read.returned_blob_bytes, CELL_BYTES as u64 + 64);
    assert_eq!(read.rows_changed + read.direct_rows_changed, 0);
    let resources = db.resources(Some(route)).unwrap();
    assert!(resources.allocation.high_water_allocated_bytes >= initial.allocated_bytes);
    assert_eq!(resources.counts.payload_bytes, CELL_BYTES as u64);
    println!("S7_COST mutation={mutation:?} allocation_calls={admission:?} cell_copies={copies:?} read={read:?} resources={resources:?} plans={:?}", db.explain_payload(source).unwrap());
}

#[test]
fn failed_atomic_job_retains_attempted_cost_and_rolls_back_logical_counts() {
    let temp = Temp::new();
    let db = Overlay::create(
        &temp.0.join("overlay.sqlite"),
        ProfileConfig {
            pager_kib: 4,
            // Schema19 no longer fits64 pages. This still leaves fewer pages
            // than the unchanged128KiB mutation needs and must fail in DML.
            max_pages: Some(96),
        },
    )
    .unwrap();
    let route = db.open_workspace([233; 32], [234; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let file = File::new(&db, source, 7, Vec::new());
    let before = db.resources(Some(route)).unwrap();
    let sql = db.diagnostics();
    let error = db
        .apply(
            source,
            &Changes {
                inodes: vec![file.inode(131072)],
                write: Some(PayloadWrite {
                    serial: 7,
                    offset: 0,
                    data: std::sync::Arc::from(pattern(131072, 9)),
                }),
                ..Changes::default()
            },
        )
        .unwrap_err();
    let work = db.diagnostics().since(&sql);
    assert!(matches!(error, OverlayError::Sql(_)), "{error:?}");
    assert!(work.total().attempts > 0 && work.total().rows_changed > 0);
    assert_eq!(db.resources(Some(route)).unwrap().counts, before.counts);
    assert!(db.inode(route, 7).unwrap().is_none());
    println!("S7_FAILED_COST error={error:?} attempted={:?} rollback_family={:?} logical_counts_preserved=true", work.total(), work.statements[StatementKind::Rollback as usize]);
}

/// Payload statements, their executions with trigger programs, the rows
/// they changed, and every statement attempt of the job.
fn window_cost(db: &Overlay, job: impl FnOnce()) -> (u64, u64, u64, u64) {
    let before = db.diagnostics();
    job();
    let work = db.diagnostics().since(&before);
    let payload = work.statements[StatementKind::Payload as usize];
    let total = work.total();
    assert_eq!(
        total.fullscan_steps + total.sorts + total.autoindex_rows + total.reprepares,
        0
    );
    (
        payload.attempts,
        payload.executions,
        payload.rows_changed,
        total.attempts,
    )
}

#[test]
fn one_write_window_costs_the_same_statements_at_either_file_size_and_beside_another_file() {
    let mut seen = Vec::new();
    for windows in [1_u64, 16] {
        let temp = Temp::new();
        let db = temp.db();
        let route = db.open_workspace([235; 32], [236; 32]).unwrap();
        let source = db.acquire_base_source(route, 1).unwrap();
        // An unrelated file with its own rows on both sides of the key.
        for serial in [6, 8] {
            File::new(&db, source, serial, Vec::new()).write(0, &pattern(WRITE_WINDOW, 2));
        }
        let mut file = File::new(&db, source, 7, Vec::new());
        let window = pattern(WRITE_WINDOW, 1);
        for at in 0..windows - 1 {
            file.write(at * WRITE_WINDOW as u64, &window);
        }
        let copies = db.payload_work();
        let last = (windows - 1) * WRITE_WINDOW as u64;
        let fresh = window_cost(&db, || file.write(last, &window));
        let rewritten = pattern(WRITE_WINDOW, 9);
        let over = window_cost(&db, || file.write(last, &rewritten));
        let first = window_cost(&db, || file.write(0, &rewritten));
        let copies = db.payload_work().since(copies);
        assert_eq!(copies.write_input_bytes, 3 * WRITE_WINDOW as u64);
        // The two overwrites: one positioning of the row handle per row.
        assert_eq!(
            (copies.in_place_writes, copies.in_place_bytes),
            (8, 2 * WRITE_WINDOW as u64)
        );
        assert_eq!((copies.cell_copy_bytes, copies.cell_zeroed_bytes), (0, 0));
        file.check();
        let rows = db.resources(Some(route)).unwrap().counts;
        seen.push((fresh, over, first, rows.payload_cells, rows.payload_bytes));
        println!(
            "R7_WRITE_WINDOW windows={windows} fresh={fresh:?} overwrite={over:?} overwrite_first={first:?} payload_rows={} payload_bytes={}",
            rows.payload_cells, rows.payload_bytes
        );
    }
    // (Payload attempts, Payload executions, Payload rows changed, attempts
    // of the whole job). A fresh window is four rows of 32 KiB: per row one
    // read of the slot's shapes and one insert with its accounting trigger
    // program, which updates two rows. Before rows of several cells this
    // was (32, 64, 96, 42): 32 cell upserts and their trigger programs.
    // An overwrite writes the four rows where they lie: four shape reads,
    // no row changed, no trigger; it was (32, 64, 96, 42) as well.
    let (first, fresh, over) = ((8, 12, 12, 19), (8, 12, 12, 18), (4, 4, 0, 14));
    assert_eq!((seen[0].0, seen[0].1, seen[0].2), (first, over, over));
    assert_eq!((seen[1].0, seen[1].1, seen[1].2), (fresh, over, over));
    // Stored rows of the file and of the two files beside it.
    assert_eq!((seen[0].3, seen[0].4), (3 * 4, 3 * 131_072));
    assert_eq!((seen[1].3, seen[1].4), (18 * 4, 18 * 131_072));
}
