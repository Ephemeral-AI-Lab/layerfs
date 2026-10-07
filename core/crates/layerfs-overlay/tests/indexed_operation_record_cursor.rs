//! Complete-key non-destructive traversal and actual scoped SQL work.
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
fn scope(db: &Overlay, tag: u8) -> IndexedOperationRecordScope {
    let route = db.open_workspace([tag; 32], [tag + 1; 32]).unwrap();
    IndexedOperationRecordScope {
        owner: db.acquire_operation(route, 1).unwrap(),
        file_scope: u64::MAX,
    }
}

#[test]
fn sealed_pass_preserves_all_records_across_windows_and_exact_prefixes() {
    let temp = Temp::new();
    let db = temp.db();
    let scope = scope(&db, 181);
    for start in [0, 64, 128] {
        let end = (start + 64).min(130);
        let rows: Vec<_> = (start..end).map(|n| put(key(9, n), vec![])).collect();
        apply(&db, scope, &rows);
    }
    let max = IndexedOperationRecordKey {
        kind: 9,
        key: [255; 32],
    };
    apply(&db, scope, &[put(max, vec![77; 8192])]);
    let other_file = IndexedOperationRecordScope {
        file_scope: 0,
        ..scope
    };
    let other_owner = IndexedOperationRecordScope {
        owner: db.acquire_operation(scope.owner.route(), 2).unwrap(),
        ..scope
    };
    apply(&db, other_file, &[put(key(9, 0), vec![1])]);
    apply(&db, other_owner, &[put(key(9, 0), vec![2])]);
    apply(&db, scope, &[put(key(10, 0), vec![3])]);
    let counts = db.resources(None).unwrap().counts;
    let plans = db
        .explain_indexed_operation_record(scope, key(9, 63), key(9, 63).key)
        .unwrap();
    assert!(plans.iter().any(|p| p.starts_with("keys-after:")
        && p.contains("SEARCH indexed_operation_record USING PRIMARY KEY")
        && p.contains("key>?")));
    assert!(plans.iter().any(|p| p.starts_with("keys-after-vm:")));
    assert!(!plans.iter().any(|p| p.contains("USE TEMP B-TREE")));
    let mut seen = Vec::new();
    let mut after = None;
    let mut work = Vec::new();
    for expected in [64, 64, 3, 0] {
        let before = db.diagnostics();
        let keys = db
            .indexed_operation_record_keys_after(scope, 9, after)
            .unwrap();
        let sql =
            db.diagnostics().since(&before).statements[StatementKind::OperationRecord as usize];
        assert_eq!(keys.len(), expected);
        assert!(keys.capacity() <= PAGE_ROWS);
        assert_eq!((sql.attempts, sql.executions), (1, 1));
        assert_eq!(sql.rows_returned, expected as u64);
        assert_eq!(sql.returned_blob_bytes, (expected * 32) as u64);
        assert_eq!(sql.bound_bytes, if after.is_none() { 32 } else { 64 });
        assert_eq!(
            (
                sql.fullscan_steps,
                sql.sorts,
                sql.autoindex_rows,
                sql.reprepares
            ),
            (0, 0, 0, 0)
        );
        assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(keys.iter().all(|k| after.is_none_or(|a| *k > a)));
        if let Some(last) = keys.last() {
            after = Some(*last);
        }
        seen.extend(keys);
        work.push(sql);
    }
    let mut expected: Vec<_> = (0..130).map(|n| key(9, n).key).collect();
    expected.push(max.key);
    assert_eq!(seen, expected);
    assert_eq!(
        db.resources(None).unwrap().counts,
        counts,
        "enumeration changes no record"
    );
    assert_eq!(
        db.indexed_operation_record_get(scope, max).unwrap(),
        Some(vec![77; 8192])
    );
    assert_eq!(
        db.indexed_operation_record_keys_after(scope, 9, None)
            .unwrap()[0],
        [0; 32]
    );
    println!("INDEXED_CURSOR_WORK windows={work:?} plans={plans:?}");
    db.release_operation(scope.owner).unwrap();
    db.release_operation(other_owner.owner).unwrap();
}

#[test]
fn full_binary_exclusive_boundary_and_restart_handle_lower_insertions() {
    let temp = Temp::new();
    let db = temp.db();
    let scope = scope(&db, 184);
    let zero = key(7, 0);
    let low = key(7, 9);
    let mut leading = [0; 32];
    leading[0] = 1;
    let high = IndexedOperationRecordKey {
        kind: 7,
        key: leading,
    };
    let max = IndexedOperationRecordKey {
        kind: 7,
        key: [255; 32],
    };
    apply(
        &db,
        scope,
        &[
            put(zero, vec![]),
            put(low, vec![]),
            put(high, vec![]),
            put(max, vec![]),
        ],
    );
    assert_eq!(
        db.indexed_operation_record_keys_after(scope, 7, Some(low.key))
            .unwrap(),
        vec![high.key, max.key]
    );
    assert!(db
        .indexed_operation_record_keys_after(scope, 7, Some(max.key))
        .unwrap()
        .is_empty());
    let absent_boundary = key(7, 4);
    assert_eq!(
        db.indexed_operation_record_keys_after(scope, 7, Some(absent_boundary.key))
            .unwrap(),
        vec![low.key, high.key, max.key]
    );
    let inserted = key(7, 2);
    apply(&db, scope, &[put(inserted, vec![])]);
    // A pass sealed before insertion cannot claim this new lower member. The
    // owner's next phase restarts from None rather than skipping it forever.
    assert_eq!(
        db.indexed_operation_record_keys_after(scope, 7, Some(low.key))
            .unwrap(),
        vec![high.key, max.key]
    );
    assert_eq!(
        db.indexed_operation_record_keys_after(scope, 7, None)
            .unwrap(),
        vec![zero.key, inserted.key, low.key, high.key, max.key]
    );
    assert_eq!(
        db.indexed_operation_record_first_keys(scope, 7, zero.key)
            .unwrap()[0],
        inserted.key
    );
    db.release_operation(scope.owner).unwrap();
    assert!(matches!(
        db.indexed_operation_record_keys_after(scope, 7, None),
        Err(OverlayError::Stale)
    ));
}
