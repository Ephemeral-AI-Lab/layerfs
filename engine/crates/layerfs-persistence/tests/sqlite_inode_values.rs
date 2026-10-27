//! The stored inode value table: one row an ordinal, read in one statement
//! execution, written with the Save's own publication, never replaced, and
//! refused rather than misread when the Store predates it.
mod support;
use layerfs_content::{
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
    },
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{
    location::InodeValueRow,
    port::{PackPersistence, PersistenceError, Publication},
    Storage, StoragePolicy,
};
use support::Temp;

const BINDING: &[u8] = b"inode-values";
fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}
fn create(path: &std::path::Path) -> Handles {
    Handles::create(
        config(path),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            incarnation: 1,
            cursor_key: [73; 32],
        },
    )
    .unwrap()
}
fn value(seed: u64) -> [u8; 73] {
    encode_inode_value(InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&seed.to_le_bytes()),
        metadata_root: layerfs_content::object::inode_leaf::InodeMetadata::Root(
            ObjectId::for_bytes(b"metadata"),
        ),
    })
}
fn rows(ordinals: impl IntoIterator<Item = u32>) -> Vec<InodeValueRow> {
    ordinals
        .into_iter()
        .map(|ordinal| InodeValueRow {
            ordinal,
            value: value(u64::from(ordinal)),
        })
        .collect()
}
fn publish(handles: &Handles, rows: Vec<InodeValueRow>) -> Result<(), PersistenceError> {
    handles
        .storage
        .publish(&Publication {
            inode_values: rows,
            ..Publication::default()
        })
        .map(|_| ())
}

#[test]
fn values_come_back_in_ordinal_order_from_one_statement_of_one_transaction() {
    let temp = Temp::new("inode-values-read");
    let handles = create(&temp.join("store.sqlite"));
    // A run written together, a gap, and scattered single rows.
    let stored: Vec<u32> = (1..=300).chain([1_000, 1_002, 70_000]).collect();
    publish(&handles, rows(stored.clone())).unwrap();
    for wanted in [
        vec![1],
        (1..=300).collect::<Vec<u32>>(),
        vec![5, 6, 7, 250, 1_000, 1_002, 70_000],
        // Ordinals with no row are omitted, between and beyond stored ones.
        vec![299, 300, 301, 999, 1_000, 1_001, 1_002, 80_000],
    ] {
        let before = handles.storage.diagnostics().unwrap();
        let mut found = vec![InodeValueRow {
            ordinal: 9,
            value: [0; 73],
        }];
        handles.storage.inode_values(&wanted, &mut found).unwrap();
        let after = handles.storage.diagnostics().unwrap();
        let expected: Vec<_> = rows(wanted.iter().copied().filter(|n| stored.contains(n)));
        assert_eq!(found, expected, "{wanted:?}");
        // One read and no BEGIN/COMMIT: stored inode values never change.
        assert_eq!(after.statements - before.statements, 1, "{wanted:?}");
        assert_eq!(after.transactions - before.transactions, 0, "{wanted:?}");
        assert_eq!(after.fullscan_steps - before.fullscan_steps, 0);
        assert_eq!(after.sorts - before.sorts, 0);
    }
    // The second execution of the same statement is not prepared again.
    let before = handles.storage.diagnostics().unwrap();
    handles
        .storage
        .inode_values(&[2, 3], &mut Vec::new())
        .unwrap();
    let after = handles.storage.diagnostics().unwrap();
    assert_eq!(after.reprepares - before.reprepares, 0);
}

#[test]
fn a_malformed_set_is_refused_before_any_statement() {
    let temp = Temp::new("inode-values-bounds");
    let handles = create(&temp.join("store.sqlite"));
    publish(&handles, rows(1..=4)).unwrap();
    let too_many: Vec<u32> = (1..=layerfs_storage::policy::READ_OBJECT_LIMIT as u32 + 1).collect();
    for wanted in [vec![0, 1], vec![2, 2], vec![3, 2], too_many] {
        let mut found = Vec::new();
        assert_eq!(
            handles.storage.inode_values(&wanted, &mut found),
            Err(PersistenceError::Malformed)
        );
        assert!(found.is_empty());
    }
    assert_eq!(
        publish(&handles, rows([9, 9])),
        Err(PersistenceError::Malformed)
    );
    assert_eq!(
        publish(&handles, rows([0])),
        Err(PersistenceError::Malformed)
    );
}

#[test]
fn a_stored_value_is_never_replaced_and_a_refused_batch_leaves_no_row() {
    let temp = Temp::new("inode-values-once");
    let handles = create(&temp.join("store.sqlite"));
    publish(&handles, rows(1..=3)).unwrap();
    // One ordinal of the batch already has a value: the whole batch is
    // refused, including the rows before and after it.
    let mut batch = rows([10, 2, 11]);
    batch[1].value = value(999);
    assert!(publish(&handles, batch).is_err());
    let mut found = Vec::new();
    handles
        .storage
        .inode_values(&[1, 2, 3, 10, 11], &mut found)
        .unwrap();
    assert_eq!(found, rows(1..=3));
}

fn leaf(seeds: &[u64]) -> FinalizedObject {
    let rows: Vec<_> = seeds
        .iter()
        .enumerate()
        .map(|(row, seed)| InodeLeafRow {
            serial: row as u64 + 1,
            value: value(*seed),
        })
        .collect();
    FinalizedObject::new(
        ObjectRole::InodeLeaf,
        InodeLeaf {
            subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
            rows,
        }
        .encode()
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn a_row_changed_in_the_file_fails_the_read_of_its_leaf() {
    let temp = Temp::new("inode-values-corrupt");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let object = leaf(&[1, 2, 3, 4]);
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    let saved = save.finish().unwrap();
    assert_eq!(saved.pool.new_values, 4);
    let read = || {
        Storage::new(handles.storage.clone())
            .unwrap()
            .reader()
            .unwrap()
            .read_objects(&[object.id()])
    };
    assert_eq!(read().unwrap(), vec![object.canonical().to_vec()]);
    let db = rusqlite::Connection::open(&path).unwrap();
    let stored: Vec<(i64, Vec<u8>)> = db
        .prepare("SELECT ordinal,value FROM inode_value ORDER BY ordinal")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(stored.len(), 4);
    assert!(stored.iter().all(|(_, value)| value.len() == 73));
    // A well-formed value of the right length that is not the one saved.
    db.execute(
        "UPDATE inode_value SET value=?1 WHERE ordinal=?2",
        rusqlite::params![value(77).to_vec(), stored[2].0],
    )
    .unwrap();
    let error = read().unwrap_err();
    assert!(
        error.to_string().contains("pooled canonical identity"),
        "{error}"
    );
    // The same for a deleted row.
    db.execute(
        "DELETE FROM inode_value WHERE ordinal=?1",
        rusqlite::params![stored[2].0],
    )
    .unwrap();
    let error = read().unwrap_err();
    assert!(
        error.to_string().contains("metadata ordinal missing"),
        "{error}"
    );
    // The table itself refuses a value of another length.
    assert!(db
        .execute(
            "INSERT INTO inode_value(ordinal,value) VALUES(?1,?2)",
            rusqlite::params![stored[2].0, vec![0u8; 72]],
        )
        .is_err());
}

#[test]
fn a_store_of_the_value_group_format_is_refused_at_open() {
    // Versions 1 to 6 are the Stores that kept values in pooled packs behind
    // `metadata_value_group`. Neither the old version number nor the old
    // table is accepted, with or without the other. Versions 7 and 13 are
    // the formats before #327's commit-v3 records, refused without migration.
    for change in [
        "PRAGMA user_version=1",
        "PRAGMA user_version=4",
        "PRAGMA user_version=6",
        "PRAGMA user_version=7",
        "PRAGMA user_version=13",
        "CREATE TABLE metadata_value_group (first_ordinal INTEGER PRIMARY KEY) STRICT",
        "DROP TABLE inode_value",
        "DROP TABLE inode_value; CREATE TABLE metadata_value_group (first_ordinal INTEGER PRIMARY KEY) STRICT; PRAGMA user_version=1",
    ] {
        let temp = Temp::new("inode-values-old-store");
        let path = temp.join("store.sqlite");
        drop(create(&path));
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(change)
            .unwrap();
        for opened in [
            Handles::open_writable(config(&path), BINDING, [73; 32]).err(),
            Handles::open_read_only(config(&path), BINDING, [73; 32]).err(),
        ] {
            assert_eq!(opened, Some(PersistenceError::Malformed), "{change}");
        }
    }
    // An unchanged Store of this format opens.
    let temp = Temp::new("inode-values-current-store");
    let path = temp.join("store.sqlite");
    drop(create(&path));
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 16);
    Handles::open_writable(config(&path), BINDING, [73; 32]).unwrap();
}
