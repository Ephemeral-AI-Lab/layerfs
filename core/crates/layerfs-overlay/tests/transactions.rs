//! Public proofs that an atomic job begins at its first writing statement,
//! pays physical admission only then, and mints engine-wide owner identities.
mod payload_support;
use layerfs_overlay::*;
use payload_support::Temp;

const CONTROL: [StatementKind; 4] = [
    StatementKind::Startup,
    StatementKind::Begin,
    StatementKind::Commit,
    StatementKind::Rollback,
];
fn key(number: u64) -> IndexedOperationRecordKey {
    let mut key = [0; 32];
    key[24..].copy_from_slice(&number.to_be_bytes());
    IndexedOperationRecordKey { kind: 1, key }
}
fn change(
    number: u64,
    expected: Option<&[u8]>,
    value: Option<&[u8]>,
) -> IndexedOperationRecordChange {
    IndexedOperationRecordChange {
        key: key(number),
        expected: match expected {
            Some(bytes) => OperationRecordExpectedValue::ExactBytes(bytes.to_vec()),
            None => OperationRecordExpectedValue::Missing,
        },
        value: value.map(<[u8]>::to_vec),
    }
}
fn scope(db: &Overlay, tag: u8) -> IndexedOperationRecordScope {
    let route = db.open_workspace([tag; 32], [tag + 1; 32]).unwrap();
    IndexedOperationRecordScope {
        owner: db.acquire_operation(route, 1).unwrap(),
        file_scope: 0,
    }
}
fn applied(
    db: &Overlay,
    scope: IndexedOperationRecordScope,
    changes: &[IndexedOperationRecordChange],
) {
    assert_eq!(
        db.indexed_operation_record_apply(scope, changes).unwrap(),
        IndexedOperationRecordApply::Applied
    );
}
fn file() -> Inode {
    Inode {
        serial: 2,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: 4,
        mtime_nanoseconds: 5,
        nlink: 1,
        size: 7,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
    }
}

/// The engine asks SQLite whether a prepared statement can write. These are
/// the answers of the linked SQLite that the lazy BEGIN depends on.
#[test]
fn linked_sqlite_classifies_reads_and_writes_as_the_engine_expects() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE t(a)").unwrap();
    for (sql, readonly) in [
        ("SELECT a FROM t", true),
        ("PRAGMA main.freelist_count", true),
        ("PRAGMA main.page_count", true),
        ("EXPLAIN QUERY PLAN SELECT a FROM t", true),
        ("COMMIT", true),
        ("ROLLBACK", true),
        ("BEGIN IMMEDIATE", false),
        ("INSERT INTO t VALUES(1)", false),
        ("UPDATE t SET a=2 WHERE a=0", false),
        ("DELETE FROM t WHERE a=0", false),
        ("INSERT INTO t VALUES(1) RETURNING a", false),
    ] {
        assert_eq!(
            connection.prepare(sql).unwrap().readonly(),
            readonly,
            "{sql}"
        );
    }
}

#[test]
fn a_read_only_atomic_job_begins_commits_and_admits_nothing() {
    let temp = Temp::new();
    let db = temp.db();
    let scope = scope(&db, 1);
    applied(&db, scope, &[change(1, None, Some(b"one"))]);
    let sql = db.diagnostics();
    let physical = db.allocation_work();
    // The guard does not hold, so the job reads and changes nothing.
    assert_eq!(
        db.indexed_operation_record_apply(scope, &[change(1, None, Some(b"two"))])
            .unwrap(),
        IndexedOperationRecordApply::NotApplied {
            index: 0,
            key: key(1),
            actual: Some(b"one".to_vec()),
        }
    );
    let work = db.diagnostics().since(&sql);
    for kind in CONTROL {
        assert_eq!(
            work.statements[kind as usize],
            StatementWork::default(),
            "{kind:?}"
        );
    }
    assert!(work.total().executions > 0);
    assert_eq!(work.total().rows_changed, 0);
    assert_eq!(
        db.allocation_work().since(physical),
        AllocationWork::default()
    );
    assert_eq!(
        db.indexed_operation_record_get(scope, key(1)).unwrap(),
        Some(b"one".to_vec())
    );
}

#[test]
fn reads_then_writes_are_one_transaction_begun_at_the_first_write() {
    let temp = Temp::new();
    let db = temp.db();
    let scope = scope(&db, 3);
    applied(&db, scope, &[change(1, None, Some(b"one"))]);
    let sql = db.diagnostics();
    let physical = db.allocation_work();
    // Both guards are read before the first write. They see the state the
    // transaction then changes: one present value and one missing key.
    applied(
        &db,
        scope,
        &[
            change(1, Some(b"one"), Some(b"two")),
            change(2, None, Some(b"three")),
        ],
    );
    let work = db.diagnostics().since(&sql);
    let executions = |kind: StatementKind| work.statements[kind as usize].executions;
    assert_eq!(executions(StatementKind::Begin), 1);
    assert_eq!(executions(StatementKind::Commit), 1);
    assert_eq!(executions(StatementKind::Rollback), 0);
    // The freelist cookie of the one admission.
    assert_eq!(executions(StatementKind::Startup), 1);
    let admission = db.allocation_work().since(physical);
    assert_eq!(admission.freelist_queries, 1);
    assert_eq!(admission.admitted_jobs, 1);
    assert_eq!(admission.refusals, 0);
    assert!(admission.attempts <= 1);
    // One observation admits; an allocation adds its own readback.
    assert_eq!(admission.observations, 1 + admission.attempts);
    assert_eq!(
        db.indexed_operation_record_get(scope, key(1)).unwrap(),
        Some(b"two".to_vec())
    );
    assert_eq!(
        db.indexed_operation_record_get(scope, key(2)).unwrap(),
        Some(b"three".to_vec())
    );
}

#[test]
fn a_failure_after_the_first_write_rolls_back_and_one_before_it_has_nothing_to_roll_back() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([5; 32], [6; 32]).unwrap();
    let released = db.acquire_base_source(route, 7).unwrap();
    db.release_base_source(released).unwrap();
    let ticket = db.publish(route, &file(), None, None).unwrap();
    let sql = db.diagnostics();
    let physical = db.allocation_work();
    // The reply attempt is deleted first; the source that is no longer held
    // then fails the job, and the deletion is rolled back with it.
    assert!(matches!(
        db.reply_attempted_and_release(ticket, released),
        Err(OverlayError::Stale)
    ));
    let work = db.diagnostics().since(&sql);
    let executions = |kind: StatementKind| work.statements[kind as usize].executions;
    assert_eq!(executions(StatementKind::Begin), 1);
    assert_eq!(executions(StatementKind::Rollback), 1);
    assert_eq!(executions(StatementKind::Commit), 0);
    assert!(work.total().rows_changed > 0);
    let admission = db.allocation_work().since(physical);
    assert_eq!(admission.admitted_jobs, 1);
    // Admission, an allocation's readback, and the one after the rollback.
    assert_eq!(admission.observations, 2 + admission.attempts);
    // The ticket is still owed and settles exactly once afterwards.
    assert!(!db.capture_ready(route).unwrap());
    db.reply_attempted(ticket).unwrap();
    assert!(db.capture_ready(route).unwrap());

    let scope = IndexedOperationRecordScope {
        owner: db.acquire_operation(route, 1).unwrap(),
        file_scope: 0,
    };
    db.release_operation(scope.owner).unwrap();
    let sql = db.diagnostics();
    let physical = db.allocation_work();
    // The released owner fails the job in its reads, before any write.
    assert!(matches!(
        db.indexed_operation_record_apply(scope, &[change(1, None, Some(b"one"))]),
        Err(OverlayError::Stale)
    ));
    let work = db.diagnostics().since(&sql);
    for kind in CONTROL {
        assert_eq!(
            work.statements[kind as usize],
            StatementWork::default(),
            "{kind:?}"
        );
    }
    assert!(work.total().executions > 0);
    assert_eq!(work.total().rows_changed, 0);
    assert_eq!(
        db.allocation_work().since(physical),
        AllocationWork::default()
    );
}

#[test]
fn consecutive_writes_that_do_not_grow_the_file_allocate_at_most_once() {
    let temp = Temp::new();
    let db = temp.db();
    let scope = scope(&db, 7);
    applied(&db, scope, &[change(1, None, Some(b"one"))]);
    let length = db.allocation().unwrap().logical_bytes;
    let physical = db.allocation_work();
    applied(&db, scope, &[change(1, Some(b"one"), Some(b"two"))]);
    let first = db.allocation_work().since(physical);
    applied(&db, scope, &[change(1, Some(b"two"), Some(b"six"))]);
    let both = db.allocation_work().since(physical);
    assert_eq!(both.admitted_jobs, 2);
    assert_eq!(both.freelist_queries, 2);
    assert_eq!(both.refusals, 0);
    // The file had grown into the startup range, so the first admission may
    // establish the range again. The second finds it whole.
    assert!(both.attempts <= 1, "{both:?}");
    assert_eq!(both.attempts, first.attempts);
    assert_eq!(both.observations, 2 + both.attempts);
    let after = db.allocation().unwrap();
    assert_eq!(after.logical_bytes, length);
    // The guarantee itself: reusable pages plus the reserved tail cover a job.
    let (_, free) = db.pages().unwrap();
    assert!(after.reserved_tail_bytes + free * 4096 >= MUTATION_GROWTH + CLEANUP_HEADROOM);
}

#[test]
fn owner_identities_are_unique_and_increasing_across_namespaces() {
    let temp = Temp::new();
    let db = temp.db();
    let routes = [
        db.open_workspace([9; 32], [10; 32]).unwrap(),
        db.open_workspace([11; 32], [10; 32]).unwrap(),
    ];
    let sources = routes.map(|route| db.acquire_base_source(route, 7).unwrap());
    let mut minted = Vec::new();
    for request in 1..=3 {
        for source in sources {
            let open = db.open_file(source, request, &file(), false).unwrap();
            minted.push(open.owner_id());
        }
    }
    // One counter serves the engine; the first identity is 1.
    assert_eq!(minted, [1, 2, 3, 4, 5, 6]);
    // A repeated request fails after its identity was minted. The identity
    // is not handed out again.
    assert!(db.open_file(sources[0], 1, &file(), false).is_err());
    let next = db.open_file(sources[1], 4, &file(), false).unwrap();
    assert!(next.owner_id() > 6);
    assert_eq!(
        db.retained_file(routes[1], 4)
            .unwrap()
            .map(OpenFile::owner_id),
        Some(next.owner_id())
    );
}
