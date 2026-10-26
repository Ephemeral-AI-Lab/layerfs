//! #355: merge build records in the Store file. Each merge sees only its own
//! records, guarded batches apply whole or not at all, `end` removes them,
//! and a writable open removes only owners idle past the lease; a removed
//! owner's later calls fail rather than read its records as absent. Records
//! stay resident up to `RESIDENT_BYTES` (#364); past it batches wait in a
//! bounded buffer until it is full or a key window is asked.
mod support;
use layerfs_content::{
    EditRecordApply, EditRecordChange, EditRecordExpected, EditRecordKey, IndexedEditBacking,
};
use layerfs_persistence::{
    Handles, PersistenceConfig, SqlitePersistenceProfile, BUFFER_BYTES, RESIDENT_BYTES,
};
use layerfs_storage::port::PersistenceError;
use support::*;

fn writable(path: &std::path::Path) -> Handles {
    Handles::open_writable(
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        BINDING,
        [71; 32],
    )
    .expect("open writable")
}

fn key(kind: u32, n: u8) -> EditRecordKey {
    EditRecordKey { kind, key: [n; 32] }
}

fn put(n: u8, value: &[u8]) -> EditRecordChange {
    EditRecordChange {
        key: key(1, n),
        expected: EditRecordExpected::Missing,
        value: Some(value.to_vec()),
    }
}

/// Batches of 64 KiB values past `RESIDENT_BYTES`, then a key window: the
/// merge's records are then all in the file.
fn spill_to_file(records: &mut layerfs_persistence::MergeRecords, kind: u32) -> u8 {
    let big = vec![7; BUFFER_BYTES / 4];
    let count = (RESIDENT_BYTES / big.len() + 1) as u8;
    for n in 0..count {
        let change = EditRecordChange {
            key: key(kind, n),
            expected: EditRecordExpected::Missing,
            value: Some(big.clone()),
        };
        records.apply(vec![change]).unwrap();
    }
    records.keys_after(kind, None).unwrap();
    count
}

fn rows(path: &std::path::Path) -> (i64, i64) {
    let sql = rusqlite::Connection::open(path).unwrap();
    let count = |table: &str| {
        sql.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    };
    (count("merge_operation"), count("merge_record"))
}

#[test]
fn records_are_private_guarded_and_removed() {
    let t = Temp::new("merge-records");
    let path = t.join("db");
    drop(create(&path));
    let handles = writable(&path);
    let scratch = handles.storage.merge_scratch();
    let mut a = scratch.begin().unwrap();
    let mut b = scratch.begin().unwrap();
    assert_eq!(
        a.apply(vec![put(1, b"one"), put(2, b"two"), put(3, b"")])
            .unwrap(),
        EditRecordApply::Applied
    );
    assert_eq!(a.get(key(1, 2)).unwrap(), Some(b"two".to_vec()));
    assert_eq!(a.get(key(1, 3)).unwrap(), Some(Vec::new()));
    assert!(!a.contains(key(2, 1)).unwrap());
    assert_eq!(b.get(key(1, 1)).unwrap(), None, "records are per merge");
    // A failed guard changes nothing and reports the deciding record.
    let refused = a
        .apply(vec![
            EditRecordChange {
                key: key(1, 1),
                expected: EditRecordExpected::ExactBytes(b"one".to_vec()),
                value: None,
            },
            put(2, b"again"),
        ])
        .unwrap();
    assert_eq!(
        refused,
        EditRecordApply::NotApplied {
            index: 1,
            key: key(1, 2),
            actual: Some(b"two".to_vec()),
        }
    );
    assert!(a.contains(key(1, 1)).unwrap());
    assert_eq!(
        a.first_keys(1, None).unwrap(),
        vec![[1; 32], [2; 32], [3; 32]]
    );
    assert_eq!(
        a.first_keys(1, Some([1; 32])).unwrap(),
        vec![[2; 32], [3; 32]]
    );
    assert_eq!(a.keys_after(1, Some([2; 32])).unwrap(), vec![[3; 32]]);
    assert!(a.keys_after(2, None).unwrap().is_empty());
    let many: Vec<_> = (10..80).map(|n| put(n, &[n])).collect();
    assert_eq!(a.apply(many).unwrap(), EditRecordApply::Applied);
    assert_eq!(a.keys_after(1, None).unwrap().len(), 64, "one window");
    b.apply(vec![put(9, b"b")]).unwrap();
    assert!(b.keys_after(1, None).unwrap().len() == 1);
    assert_eq!(rows(&path).1, 0, "resident records are not in the file");
    // Past the resident bytes every record goes to the file at once; then a
    // batch flushes once the buffer is full, without a key window.
    let mut c = scratch.begin().unwrap();
    let spilled = spill_to_file(&mut c, 1) as i64;
    assert_eq!(rows(&path).1, spilled);
    let big = vec![9; BUFFER_BYTES / 4];
    for n in 0..4 {
        c.apply(vec![put(200 + n, &big)]).unwrap();
    }
    assert_eq!(rows(&path).1, spilled + 4);
    assert_eq!(
        c.get_many(&[key(1, 200), key(1, 250)]).unwrap(),
        vec![Some(big), None]
    );
    assert_eq!(c.keys_after(1, Some([199; 32])).unwrap().len(), 4);
    let file = c.end().unwrap();
    assert!(file.reads > 0 && file.writes >= 4, "{file:?}");
    assert!(a.failure().is_none());
    let work = a.end().unwrap();
    assert!(
        work.reads == 0 && work.writes == 2 && work.max_write_ns <= work.write_ns,
        "a resident merge reads no statement and writes only its owner: {work:?}"
    );
    assert_eq!(rows(&path), (1, 0), "only the other merge remains");
    b.end().unwrap();
    assert_eq!(rows(&path), (0, 0));
}

#[test]
fn open_sweeps_only_idle_owners() {
    let t = Temp::new("merge-records-sweep");
    let path = t.join("db");
    drop(create(&path));
    let handles = writable(&path);
    let scratch = handles.storage.merge_scratch();
    let mut idle = scratch.begin().unwrap();
    let mut live = scratch.begin().unwrap();
    let mut resident = scratch.begin().unwrap();
    // The idle and live merges' records are in the file.
    let idle_rows = spill_to_file(&mut idle, 1) as i64;
    let live_rows = spill_to_file(&mut live, 2) as i64;
    live.apply(vec![put(1, b"live")]).unwrap();
    live.keys_after(1, None).unwrap();
    resident.apply(vec![put(1, b"held")]).unwrap();
    assert_eq!(rows(&path), (3, idle_rows + live_rows + 1));
    {
        let sql = rusqlite::Connection::open(&path).unwrap();
        // The first owner made no call for longer than the lease.
        sql.execute(
            "UPDATE merge_operation SET touched=0 WHERE operation=(SELECT min(operation) FROM merge_operation)",
            [],
        )
        .unwrap();
        // So did the resident one.
        sql.execute(
            "UPDATE merge_operation SET touched=0 WHERE operation=(SELECT max(operation) FROM merge_operation)",
            [],
        )
        .unwrap();
    }
    let reopened = writable(&path);
    assert_eq!(
        rows(&path),
        (1, live_rows + 1),
        "the idle owners and their records are gone"
    );
    assert!(
        idle.get(key(1, 1)).is_err(),
        "a removed owner never reads absence"
    );
    assert_eq!(idle.failure(), Some(&PersistenceError::Missing));
    assert!(
        idle.get(key(1, 2)).is_err(),
        "the first failure ends its calls"
    );
    assert_eq!(live.get(key(1, 1)).unwrap(), Some(b"live".to_vec()));
    assert_eq!(idle.end().err(), Some(PersistenceError::Missing));
    // A resident merge whose owner is gone keeps its records, but cannot
    // write them to the file or end as if it still owned them.
    assert_eq!(resident.get(key(1, 1)).unwrap(), Some(b"held".to_vec()));
    assert_eq!(resident.end().err(), Some(PersistenceError::Missing));
    live.end().unwrap();
    drop(reopened);
    assert_eq!(rows(&path), (0, 0));
}

/// #364: a key window is a range read of the records' primary key. The old
/// window materialized its keys and scanned them, one full-scan step and one
/// sort per call; a merge of a lane with `node_modules` made 265 such calls.
#[test]
fn key_windows_read_the_primary_key_without_a_scan() {
    let t = Temp::new("merge-records-window");
    let path = t.join("db");
    drop(create(&path));
    let handles = writable(&path);
    let scratch = handles.storage.merge_scratch();
    let mut a = scratch.begin().unwrap();
    // Records in the file: keys 0..count of kind 1.
    let count = spill_to_file(&mut a, 1);
    let before = handles.storage.diagnostics().unwrap();
    let first = a.first_keys(1, Some([1; 32])).unwrap();
    let after = a.keys_after(1, Some([100; 32])).unwrap();
    let empty = a.keys_after(2, None).unwrap();
    let work = handles.storage.diagnostics().unwrap();
    assert_eq!(first.len(), 64);
    assert_eq!(&first[..2], &[[0; 32], [2; 32]]);
    assert_eq!(after, (101..count).map(|n| [n; 32]).collect::<Vec<_>>());
    assert!(empty.is_empty());
    assert_eq!(work.fullscan_steps - before.fullscan_steps, 0, "no scan");
    assert_eq!(work.sorts - before.sorts, 0, "no sort");
    a.end().unwrap();
}

/// Every answer of one record call, for comparing two merges and a model.
#[derive(Debug, PartialEq)]
enum Answer {
    Apply(EditRecordApply),
    Values(Vec<Option<Vec<u8>>>),
    Contains(bool),
    Keys(Vec<[u8; 32]>),
}

/// One step of a mixed sequence over kind 1: puts, guarded replacements and
/// deletions, refused guards, reads and key windows.
fn step(records: &mut layerfs_persistence::MergeRecords, n: u32) -> Answer {
    let k = (n.wrapping_mul(37) % 150) as u8;
    let value = vec![(n % 251) as u8; (n % 97) as usize];
    match n % 7 {
        0 | 1 => Answer::Apply(records.apply(vec![put(k, &value)]).unwrap()),
        2 => {
            let current = records.get(key(1, k)).unwrap();
            let expected = match current {
                Some(bytes) => EditRecordExpected::ExactBytes(bytes),
                None => EditRecordExpected::Missing,
            };
            let value = (n % 3 != 0).then_some(value);
            Answer::Apply(
                records
                    .apply(vec![EditRecordChange {
                        key: key(1, k),
                        expected,
                        value,
                    }])
                    .unwrap(),
            )
        }
        // A guard that is wrong whenever the record exists.
        3 => Answer::Apply(
            records
                .apply(vec![
                    EditRecordChange {
                        key: key(1, k),
                        expected: EditRecordExpected::ExactBytes(b"never".to_vec()),
                        value: None,
                    },
                    put(k + 1, b"x"),
                ])
                .unwrap(),
        ),
        4 => Answer::Values(
            records
                .get_many(&[key(1, k), key(1, k / 2), key(1, 255)])
                .unwrap(),
        ),
        5 => Answer::Contains(records.contains(key(1, k)).unwrap()),
        _ => Answer::Keys(if n % 2 == 0 {
            records.keys_after(1, Some([k; 32])).unwrap()
        } else {
            records.first_keys(1, Some([k; 32])).unwrap()
        }),
    }
}

#[test]
fn a_merge_past_the_resident_bound_answers_as_a_resident_one() {
    let t = Temp::new("merge-records-spill");
    let path = t.join("db");
    drop(create(&path));
    let handles = writable(&path);
    let scratch = handles.storage.merge_scratch();
    let mut resident = scratch.begin().unwrap();
    let mut backed = scratch.begin().unwrap();
    // Past RESIDENT_BYTES on another kind: every later call of `backed`
    // reads and writes merge_record rows; `resident` stays in memory.
    spill_to_file(&mut backed, 9);
    assert!(rows(&path).1 > 0);
    for n in 0..3_000 {
        let (a, b) = (step(&mut resident, n), step(&mut backed, n));
        assert_eq!(a, b, "step {n}");
    }
    // The same records at the end, read as whole key windows.
    let all = |records: &mut layerfs_persistence::MergeRecords| {
        let (mut out, mut after) = (Vec::new(), None);
        loop {
            let page = records.keys_after(1, after).unwrap();
            if page.is_empty() {
                return out;
            }
            after = page.last().copied();
            for k in page {
                out.push((k, records.get(EditRecordKey { kind: 1, key: k }).unwrap()));
            }
        }
    };
    let (a, b) = (all(&mut resident), all(&mut backed));
    assert!(!a.is_empty());
    assert_eq!(a, b);
    let (a, b) = (resident.end().unwrap(), backed.end().unwrap());
    assert_eq!(a.reads, 0, "the resident merge read no row");
    assert!(
        b.reads > 0 && b.flushes > 0,
        "the backed merge used the file: {b:?}"
    );
    assert_eq!(rows(&path), (0, 0));
}
