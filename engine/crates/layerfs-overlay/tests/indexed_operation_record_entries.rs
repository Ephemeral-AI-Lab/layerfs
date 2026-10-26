//! Key-ordered windows of keys and values: pages by rows and by bytes, never refuses.
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
fn scope(db: &Overlay) -> IndexedOperationRecordScope {
    db.open_workspace([7; 32]).unwrap();
    IndexedOperationRecordScope {
        owner: db.acquire_operation(1).unwrap(),
        file_scope: 3,
    }
}
type Rows = Vec<([u8; 32], Vec<u8>)>;
/// Every (key, value) of one kind, a window at a time, with the window sizes.
fn walk(db: &Overlay, scope: IndexedOperationRecordScope, kind: u32) -> (Rows, Vec<usize>) {
    let (mut all, mut sizes, mut after) = (Vec::new(), Vec::new(), None);
    loop {
        let (window, ended) = db
            .indexed_operation_record_entries_after(scope, kind, after)
            .unwrap();
        sizes.push(window.len());
        after = window.last().map(|(key, _)| *key).or(after);
        all.extend(window);
        if ended {
            return (all, sizes);
        }
    }
}

#[test]
fn windows_page_by_rows_in_key_order_within_one_scope_and_kind() {
    let temp = Temp::new();
    let db = temp.db();
    let scope = scope(&db);
    for start in [0, 64, 128] {
        let end = (start + 64).min(130);
        let rows: Vec<_> = (start..end)
            .map(|n| put(key(9, n), n.to_be_bytes().to_vec()))
            .collect();
        apply(&db, scope, &rows);
    }
    let other_file = IndexedOperationRecordScope {
        file_scope: 4,
        ..scope
    };
    apply(&db, other_file, &[put(key(9, 500), vec![1])]);
    apply(&db, scope, &[put(key(10, 0), vec![2])]);
    let (all, sizes) = walk(&db, scope, 9);
    assert_eq!(sizes, vec![64, 64, 2]);
    assert_eq!(all.len(), 130);
    for (n, (raw, value)) in all.iter().enumerate() {
        assert_eq!(*raw, key(9, n as u64).key);
        assert_eq!(*value, (n as u64).to_be_bytes().to_vec());
    }
    // An exact multiple of a window ends with one empty window.
    let (none, sizes) = walk(&db, scope, 11);
    assert!(none.is_empty());
    assert_eq!(sizes, vec![0]);
    let plans = db
        .explain_indexed_operation_record(scope, key(9, 63), key(9, 63).key)
        .unwrap();
    assert!(plans.iter().any(|p| p.starts_with("entries-after:")
        && p.contains("SEARCH commit_record USING PRIMARY KEY")
        && p.contains("key>?")));
    assert!(plans.iter().any(|p| p.starts_with("entries:")));
    assert!(!plans.iter().any(|p| p.contains("USE TEMP B-TREE")));
}

#[test]
fn a_window_stops_at_the_record_envelope_and_the_caller_continues() {
    let temp = Temp::new();
    let db = temp.db();
    let scope = scope(&db);
    // Two 30,000-byte values fit one 65,536-byte envelope, a third does not.
    let big = 30_000;
    for n in 0..5_u64 {
        apply(&db, scope, &[put(key(9, n), vec![n as u8; big])]);
    }
    apply(&db, scope, &[put(key(9, 5), vec![5])]);
    let (all, sizes) = walk(&db, scope, 9);
    assert_eq!(sizes, vec![2, 2, 2]);
    assert_eq!(all.len(), 6);
    for (n, (raw, value)) in all.iter().enumerate() {
        assert_eq!(*raw, key(9, n as u64).key);
        assert_eq!(value[0], n as u8);
    }
    // A value over half the envelope comes back alone in its window.
    let alone = IndexedOperationRecordScope {
        file_scope: 8,
        ..scope
    };
    let largest = 60_000;
    assert!(2 * largest > OPERATION_RECORD_BYTES);
    apply(&db, alone, &[put(key(9, 0), vec![1; largest])]);
    apply(&db, alone, &[put(key(9, 1), vec![2; largest])]);
    let (all, sizes) = walk(&db, alone, 9);
    assert_eq!(sizes, vec![1, 1]);
    assert_eq!(all[1].1.len(), largest);
}
