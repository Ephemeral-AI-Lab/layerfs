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
    assert_eq!(copies.cell_copy_bytes, CELL_BYTES as u64);
    assert_eq!(copies.cell_zeroed_bytes, 0);
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
