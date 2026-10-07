//! Public full-identity operation_record, atomicity, lifetime and scoped SQL observations.
mod payload_support;
use layerfs_overlay::*;
use payload_support::Temp;

fn key(kind: u32, number: u64) -> IndexedOperationRecordKey {
    let mut key = [0; 32];
    key[24..].copy_from_slice(&number.to_be_bytes());
    IndexedOperationRecordKey { kind, key }
}
fn put(key: IndexedOperationRecordKey, value: Vec<u8>) -> IndexedOperationRecordChange {
    IndexedOperationRecordChange {
        key,
        expected: OperationRecordExpectedValue::Missing,
        value: Some(value),
    }
}
fn apply(
    db: &Overlay,
    scope: IndexedOperationRecordScope,
    changes: &[IndexedOperationRecordChange],
) {
    assert_eq!(
        db.indexed_operation_record_apply(scope, changes).unwrap(),
        IndexedOperationRecordApply::Applied
    );
}
fn scope(db: &Overlay, tag: u8, request: u64, file_scope: u64) -> IndexedOperationRecordScope {
    let route = db.open_workspace([tag; 32], [tag + 1; 32]).unwrap();
    IndexedOperationRecordScope {
        owner: db.acquire_operation(route, request).unwrap(),
        file_scope,
    }
}
fn maintain(db: &Overlay, limit: usize) {
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..limit {
        let Some(step) = db.maintain(cursor).unwrap() else {
            return;
        };
        assert!(step.work.rows <= PAGE_ROWS as u64);
        assert!(step.work.data_bytes <= OPERATION_RECORD_BYTES as u64);
        cursor = step.cursor;
    }
    panic!("bounded operation_record maintenance did not finish");
}

#[test]
fn complete_keys_and_full_file_scopes_never_alias_other_custody() {
    let temp = Temp::new();
    let db = temp.db();
    let first = scope(&db, 201, 1, 0);
    let second = IndexedOperationRecordScope {
        owner: db.acquire_operation(first.owner.route(), 2).unwrap(),
        file_scope: 0,
    };
    let other_file = IndexedOperationRecordScope {
        file_scope: u64::MAX,
        ..first
    };
    let other_namespace = scope(&db, 203, 1, 0);
    let low = key(u32::MAX, 1);
    let high = IndexedOperationRecordKey {
        key: [0xff; 32],
        ..low
    };
    assert!(!db.indexed_operation_record_contains(first, low).unwrap());
    assert_eq!(db.indexed_operation_record_get(first, low).unwrap(), None);
    apply(&db, first, &[put(low, vec![]), put(high, vec![11])]);
    apply(&db, second, &[put(low, vec![12])]);
    apply(&db, other_file, &[put(low, vec![13])]);
    apply(&db, other_namespace, &[put(low, vec![14])]);
    assert!(db.indexed_operation_record_contains(first, low).unwrap());
    assert_eq!(
        db.indexed_operation_record_get(first, low).unwrap(),
        Some(vec![])
    );
    assert_eq!(
        db.indexed_operation_record_get(first, high).unwrap(),
        Some(vec![11])
    );
    for (scope, expected) in [(second, 12), (other_file, 13), (other_namespace, 14)] {
        assert_eq!(
            db.indexed_operation_record_get(scope, low).unwrap(),
            Some(vec![expected])
        );
    }
    let before = db.diagnostics();
    assert!(db.indexed_operation_record_contains(first, high).unwrap());
    let work = db.diagnostics().since(&before);
    assert_eq!(
        work.statements[StatementKind::OperationRecord as usize].returned_blob_bytes,
        0
    );
    let foreign_temp = Temp::new();
    let foreign = foreign_temp.db();
    assert!(matches!(
        foreign.indexed_operation_record_get(first, low),
        Err(OverlayError::Stale)
    ));
    let counts = db.resources(None).unwrap().counts;
    assert_eq!(
        (counts.operation_record_rows, counts.operation_record_bytes),
        (5, 4)
    );
}

#[test]
fn optional_exclusion_keeps_zero_identity_and_finds_new_lower_keys() {
    let temp = Temp::new();
    let db = temp.db();
    let scope = scope(&db, 221, 1, u64::MAX);
    let zero = key(19, 0);
    let rows: Vec<_> = (0..65).map(|n| put(key(19, n), vec![])).collect();
    apply(&db, scope, &rows);
    let before = db.diagnostics();
    let first = db.indexed_operation_record_keys(scope, 19, None).unwrap();
    let work = db.diagnostics().since(&before);
    assert_eq!(first.len(), 64);
    assert_eq!(first[0], zero.key, "None never excludes a sentinel");
    assert_eq!(first[63], key(19, 63).key);
    let operation_record = work.statements[StatementKind::OperationRecord as usize];
    assert_eq!(
        (operation_record.attempts, operation_record.executions),
        (1, 1)
    );
    assert_eq!(
        (
            operation_record.rows_returned,
            operation_record.returned_blob_bytes
        ),
        (64, 2048)
    );
    assert_eq!(operation_record.bound_bytes, 32);
    assert_eq!(operation_record.fullscan_steps, 0);
    let filtered = db
        .indexed_operation_record_keys(scope, 19, Some(zero.key))
        .unwrap();
    assert_eq!(filtered.first(), Some(&key(19, 1).key));
    assert_eq!(filtered.last(), Some(&key(19, 64).key));
    apply(
        &db,
        scope,
        &[IndexedOperationRecordChange {
            key: zero,
            expected: OperationRecordExpectedValue::ExactBytes(vec![]),
            value: None,
        }],
    );
    assert_eq!(
        db.indexed_operation_record_keys(scope, 19, None)
            .unwrap()
            .first(),
        Some(&key(19, 1).key)
    );
    apply(&db, scope, &[put(zero, vec![])]);
    assert_eq!(
        db.indexed_operation_record_keys(scope, 19, None)
            .unwrap()
            .first(),
        Some(&zero.key)
    );
    let plans = db
        .explain_indexed_operation_record(scope, zero, zero.key)
        .unwrap();
    assert!(plans
        .iter()
        .any(|p| p.starts_with("all-keys: SEARCH indexed_operation_record USING PRIMARY KEY")));
    assert!(plans.iter().any(|p| p.starts_with("all-keys-vm:")));
    println!("INDEXED_ALL_KEYS original_work={operation_record:?} plans={plans:?}");
    db.release_operation(scope.owner).unwrap();
    maintain(&db, 32);
}

#[test]
fn guarded_batches_are_atomic_capacity_bounded_and_allow_a_128_child_node_shape() {
    // Mapping-only raw drafts: two conservative 12,460-byte buffers, 128
    // reference pre/post u64 values and 258 exactly reserved descriptors.
    let mut envelope = Vec::with_capacity(258);
    envelope.push(IndexedOperationRecordChange {
        key: key(0, 0),
        expected: OperationRecordExpectedValue::ExactBytes(vec![0; 12_460]),
        value: Some(vec![1; 12_460]),
    });
    for number in 0..128 {
        envelope.push(IndexedOperationRecordChange {
            key: key(1, number),
            expected: OperationRecordExpectedValue::ExactBytes(vec![0; 8]),
            value: Some(vec![1; 8]),
        });
    }
    for number in 0..128 {
        envelope.push(put(key(2, number), vec![]));
    }
    envelope.push(put(key(3, 0), vec![]));
    let bytes = indexed_operation_record_changes_bytes(&envelope).unwrap();
    assert_eq!(
        bytes,
        24_920 + 2048 + 258 * std::mem::size_of::<IndexedOperationRecordChange>()
    );
    assert!(bytes <= OPERATION_RECORD_BYTES);
    assert_eq!(envelope.capacity(), 258);
    println!(
        "INDEXED_ENVELOPE descriptors=258 descriptor_bytes={} retained_bytes={bytes}",
        std::mem::size_of::<IndexedOperationRecordChange>()
    );
    let temp = Temp::new();
    let db = temp.db();
    let scope = scope(&db, 205, 1, 7);
    let mut changes = Vec::with_capacity(258);
    for number in 0..258 {
        changes.push(put(key(1, number), vec![1]));
    }
    assert!(indexed_operation_record_changes_bytes(&changes).unwrap() <= OPERATION_RECORD_BYTES);
    apply(&db, scope, &changes);
    let deciding = key(1, 257);
    let guarded = [
        IndexedOperationRecordChange {
            key: key(1, 0),
            expected: OperationRecordExpectedValue::ExactBytes(vec![1]),
            value: Some(vec![2, 2]),
        },
        IndexedOperationRecordChange {
            key: deciding,
            expected: OperationRecordExpectedValue::ExactBytes(vec![99]),
            value: None,
        },
    ];
    let before = db.resources(None).unwrap().counts;
    assert_eq!(
        db.indexed_operation_record_apply(scope, &guarded).unwrap(),
        IndexedOperationRecordApply::NotApplied {
            index: 1,
            key: deciding,
            actual: Some(vec![1]),
        }
    );
    assert_eq!(
        db.indexed_operation_record_get(scope, key(1, 0)).unwrap(),
        Some(vec![1])
    );
    assert_eq!(db.resources(None).unwrap().counts, before);
    apply(
        &db,
        scope,
        &[
            IndexedOperationRecordChange {
                key: key(1, 0),
                expected: OperationRecordExpectedValue::ExactBytes(vec![1]),
                value: Some(vec![2, 2]),
            },
            IndexedOperationRecordChange {
                key: deciding,
                expected: OperationRecordExpectedValue::ExactBytes(vec![1]),
                value: None,
            },
        ],
    );
    let counts = db.resources(None).unwrap().counts;
    assert_eq!(
        (counts.operation_record_rows, counts.operation_record_bytes),
        (257, 258)
    );
    for invalid in [
        vec![put(key(2, 0), vec![]), put(key(2, 0), vec![])],
        vec![put(key(2, 1), vec![]), put(key(2, 0), vec![])],
    ] {
        let before = db.diagnostics();
        assert!(matches!(
            db.indexed_operation_record_apply(scope, &invalid),
            Err(OverlayError::Invalid(
                "indexed operation record target order"
            ))
        ));
        assert_eq!(db.diagnostics(), before);
    }
    let mut retained = Vec::with_capacity(OPERATION_RECORD_BYTES);
    retained.push(1);
    let before = db.diagnostics();
    assert!(matches!(
        db.indexed_operation_record_apply(scope, &[put(key(3, 0), retained)]),
        Err(OverlayError::Invalid(
            "indexed operation record byte window"
        ))
    ));
    assert_eq!(db.diagnostics(), before);
}

#[test]
fn first_detached_window_finds_children_inserted_below_consumed_candidates() {
    let temp = Temp::new();
    let db = temp.db();
    let scope = scope(&db, 207, 1, 1);
    let changes: Vec<_> = (1..=80).map(|n| put(key(4, n), vec![])).collect();
    apply(&db, scope, &changes);
    let root = key(4, 1);
    let keys = db
        .indexed_operation_record_first_keys(scope, 4, root.key)
        .unwrap();
    assert_eq!(keys.len(), 64);
    assert_eq!(keys.first(), Some(&key(4, 2).key));
    assert_eq!(keys.last(), Some(&key(4, 65).key));
    apply(
        &db,
        scope,
        &[
            put(key(4, 0), vec![]),
            IndexedOperationRecordChange {
                key: key(4, 2),
                expected: OperationRecordExpectedValue::ExactBytes(vec![]),
                value: None,
            },
        ],
    );
    let keys = db
        .indexed_operation_record_first_keys(scope, 4, root.key)
        .unwrap();
    assert_eq!(keys.first(), Some(&key(4, 0).key));
    assert!(!keys.contains(&root.key));
    assert!(!keys.contains(&key(4, 2).key));
    assert!(db.indexed_operation_record_contains(scope, root).unwrap());
}

#[test]
fn state_exceeding_eight_mib_reclaims_in_windows_without_reusing_a_released_owner() {
    let temp = Temp::new();
    let db = temp.db();
    let first = scope(&db, 209, 1, 1);
    db.put_owned_operation_record(
        first.owner,
        &OperationRecord {
            kind: 1,
            key: 1,
            value: vec![8; OPERATION_RECORD_BYTES],
        },
    )
    .unwrap();
    for number in 0..144 {
        apply(&db, first, &[put(key(1, number), vec![9; 60_000])]);
    }
    let before = db.resources(None).unwrap().counts;
    assert_eq!(before.operation_record_rows, 145);
    assert_eq!(
        before.operation_record_bytes,
        144 * 60_000 + OPERATION_RECORD_BYTES as u64
    );
    assert!(before.operation_record_bytes > 8 * 1024 * 1024);
    db.release_operation(first.owner).unwrap();
    let second = IndexedOperationRecordScope {
        owner: db.acquire_operation(first.owner.route(), 1).unwrap(),
        ..first
    };
    assert_ne!(first.owner, second.owner);
    apply(&db, second, &[put(key(1, 0), vec![3; 7])]);
    assert!(matches!(
        db.indexed_operation_record_get(first, key(1, 0)),
        Err(OverlayError::Stale)
    ));
    assert!(db.release_operation(first.owner).is_err());
    maintain(&db, 400);
    let after = db.resources(None).unwrap().counts;
    assert_eq!(
        (after.operation_record_rows, after.operation_record_bytes),
        (1, 7)
    );
    assert_eq!(
        db.indexed_operation_record_get(second, key(1, 0)).unwrap(),
        Some(vec![3; 7])
    );
    db.close(second.owner.route()).unwrap();
    assert_eq!(
        db.cleanup_state(second.owner.route()).unwrap(),
        CleanupState::Held
    );
    assert!(db
        .indexed_operation_record_contains(second, key(1, 0))
        .unwrap());
    assert!(matches!(
        db.indexed_operation_record_apply(second, &[put(key(2, 0), vec![])]),
        Err(OverlayError::Closed)
    ));
    db.release_operation(second.owner).unwrap();
    maintain(&db, 50);
    for _ in 0..100 {
        if db.reclaim_closed(0).unwrap().unwrap().done {
            let counts = db.resources(None).unwrap().counts;
            assert_eq!(
                (
                    counts.namespaces,
                    counts.operation_record_rows,
                    counts.operation_record_bytes
                ),
                (0, 0, 0)
            );
            return;
        }
    }
    panic!("bounded terminal operation_record reclamation did not finish");
}

#[test]
fn exact_plans_correlate_with_bounded_executed_point_and_window_work() {
    for population in [128, 1024, 4096] {
        let temp = Temp::new();
        let db = temp.db();
        let scope = scope(&db, 211, 1, 1);
        let other = IndexedOperationRecordScope {
            file_scope: u64::MAX,
            ..scope
        };
        for start in (0..population).step_by(128) {
            let changes: Vec<_> = (start..start + 128)
                .map(|n| put(key(1, n), vec![]))
                .collect();
            apply(&db, other, &changes);
        }
        apply(&db, scope, &[put(key(1, 7), vec![7; 8192])]);
        let changes: Vec<_> = (0..65).map(|n| put(key(2, n), vec![])).collect();
        apply(&db, scope, &changes);
        let plans = db
            .explain_indexed_operation_record(scope, key(1, 7), key(2, 0).key)
            .unwrap();
        for label in [
            "contains",
            "get",
            "delete",
            "keys",
            "operation-cleanup",
            "namespace-cleanup",
        ] {
            assert!(
                plans.iter().any(|p| p.starts_with(&format!("{label}:"))
                    && p.contains("SEARCH indexed_operation_record USING PRIMARY KEY")),
                "{plans:?}"
            );
        }
        assert!(!plans.iter().any(|p| p.contains("USE TEMP B-TREE")));
        assert!(plans
            .iter()
            .any(|p| p.starts_with("put-vm:") && p.contains("Program")));
        let before = db.diagnostics();
        assert!(db
            .indexed_operation_record_contains(scope, key(1, 7))
            .unwrap());
        let contains = db.diagnostics().since(&before);
        let before = db.diagnostics();
        assert_eq!(
            db.indexed_operation_record_get(scope, key(1, 7)).unwrap(),
            Some(vec![7; 8192])
        );
        let get = db.diagnostics().since(&before);
        let before = db.diagnostics();
        let window = db
            .indexed_operation_record_first_keys(scope, 2, key(2, 0).key)
            .unwrap();
        let keys = db.diagnostics().since(&before);
        assert_eq!(window.len(), 64);
        let operation_record = StatementKind::OperationRecord as usize;
        assert_eq!(contains.statements[operation_record].returned_blob_bytes, 0);
        assert_eq!(get.statements[operation_record].returned_blob_bytes, 8192);
        assert_eq!(
            keys.statements[operation_record].returned_blob_bytes,
            64 * 32
        );
        assert!(contains.statements[operation_record].vm_steps < 128);
        assert!(get.statements[operation_record].vm_steps < 128);
        assert!(keys.statements[operation_record].vm_steps < 2048);
        for work in [contains, get, keys] {
            let total = work.total();
            assert_eq!(
                (
                    total.fullscan_steps,
                    total.sorts,
                    total.autoindex_rows,
                    total.reprepares
                ),
                (0, 0, 0, 0)
            );
        }
        println!("INDEXED_OPERATION_RECORD population={population} contains={contains:?} get={get:?} keys={keys:?} plans={plans:?}; indexed visited-row counter unavailable; VM/returned observations are separate");
    }
}
