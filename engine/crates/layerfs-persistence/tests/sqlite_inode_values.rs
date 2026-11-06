//! The stored inode value blocks (#91): many consecutive ordinals' values in
//! one row, read in one statement execution, written with the Save's own
//! publication, never replaced, every 73-byte value back exactly, and
//! refused rather than misread when the Store predates them.
mod support;
use layerfs_content::{
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
    },
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
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
fn create(path: &std::path::Path) -> SqliteHandles {
    SqliteHandles::create(
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
fn publish(handles: &SqliteHandles, rows: Vec<InodeValueRow>) -> Result<(), PersistenceError> {
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
    let stored: Vec<(i64, i64, Vec<u8>)> = db
        .prepare("SELECT first_ordinal,last_ordinal,body FROM inode_value_block ORDER BY 1")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    // The four values are one block.
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].1 - stored[0].0, 3);
    // One changed byte of the block fails its checksum.
    let mut body = stored[0].2.clone();
    body[40] ^= 1;
    db.execute(
        "UPDATE inode_value_block SET body=?1 WHERE first_ordinal=?2",
        rusqlite::params![body, stored[0].0],
    )
    .unwrap();
    assert!(read().is_err());
    // A deleted block leaves its ordinals missing.
    db.execute(
        "DELETE FROM inode_value_block WHERE first_ordinal=?1",
        rusqlite::params![stored[0].0],
    )
    .unwrap();
    let error = read().unwrap_err();
    assert!(
        error.to_string().contains("metadata ordinal missing"),
        "{error}"
    );
    // The table itself refuses a body past one block and a run past 255.
    for (last, body) in [
        (stored[0].1, vec![1u8; 4001]),
        (stored[0].0 + 255, vec![1u8; 64]),
    ] {
        assert!(db
            .execute(
                "INSERT INTO inode_value_block(first_ordinal,last_ordinal,body) VALUES(?1,?2,?3)",
                rusqlite::params![stored[0].0, last, body],
            )
            .is_err());
    }
}

/// Values of every shape: both canonical forms, inline values with nonzero
/// padding and other bytes no grammar accepts, all of which come back
/// byte for byte.
fn odd_value(seed: u32) -> [u8; 73] {
    let mut value = [0u8; 73];
    for (index, byte) in value.iter_mut().enumerate() {
        *byte = (seed as usize * 31 + index * 7) as u8;
    }
    match seed % 6 {
        0 => value[0] = 0xff,
        1 => {
            value[0] = 0x81;
            value[57..].fill(0);
            value[1..9].copy_from_slice(&u64::MAX.to_be_bytes());
            value[45..53].copy_from_slice(&i64::MIN.to_be_bytes());
        }
        2 => {
            value[0] = 0x82;
            value[57..].fill(0);
            value[45..53].copy_from_slice(&i64::MAX.to_be_bytes());
        }
        3 => value[0] = 0x83, // nonzero padding: stored whole
        4 => value[0] = 0x05,
        _ => {
            value[0] = 0x81;
            value[57..].fill(0);
            value[41..45].copy_from_slice(&seed.to_be_bytes());
        }
    }
    value
}

#[test]
fn every_value_comes_back_exactly_from_blocks_of_at_most_4000_bytes() {
    let temp = Temp::new("inode-values-exact");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let stored: Vec<InodeValueRow> = (1..=700)
        .chain(900..=1_200)
        .map(|ordinal| InodeValueRow {
            ordinal,
            value: if ordinal % 2 == 0 {
                odd_value(ordinal)
            } else {
                value(u64::from(ordinal))
            },
        })
        .collect();
    publish(&handles, stored.clone()).unwrap();
    let wanted: Vec<u32> = stored.iter().map(|row| row.ordinal).collect();
    let mut found = Vec::new();
    for part in wanted.chunks(512) {
        let mut some = Vec::new();
        handles.storage.inode_values(part, &mut some).unwrap();
        found.extend(some);
    }
    assert_eq!(found, stored);
    let db = rusqlite::Connection::open(&path).unwrap();
    let blocks: Vec<(i64, i64, i64)> = db
        .prepare("SELECT first_ordinal,last_ordinal,length(body) FROM inode_value_block ORDER BY 1")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    // No block spans the gap, none is past 4,000 bytes, and together they
    // hold every ordinal once.
    assert!(blocks
        .iter()
        .all(|b| b.2 <= 4_000 && (b.1 < 900 || b.0 >= 900)));
    assert_eq!(
        blocks.iter().map(|b| b.1 - b.0 + 1).sum::<i64>(),
        stored.len() as i64
    );
    assert!(blocks.len() < stored.len() / 40, "{}", blocks.len());
}

#[test]
fn a_store_of_the_value_group_format_is_refused_at_open() {
    // Versions 1 to 6 are the Stores that kept values in pooled packs behind
    // `metadata_value_group`. Neither the old version number nor the old
    // table is accepted, with or without the other. Versions 7 and 13 are
    // the formats before #327's commit-v3 records, refused without migration.
    // Versions 16 to 18 and 22 to 24 are schema version 1, one row per
    // value, refused since #91 without conversion.
    // Schema version 1 is refused with the offline conversion named.
    for version in [16, 17, 18, 22, 23, 24] {
        let temp = Temp::new("inode-values-v1-store");
        let path = temp.join("store.sqlite");
        drop(create(&path));
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(&format!("PRAGMA user_version={version}"))
            .unwrap();
        for opened in [
            SqliteHandles::open_writable(config(&path), BINDING, [73; 32]).err(),
            SqliteHandles::open_read_only(config(&path), BINDING, [73; 32]).err(),
        ] {
            match opened {
                Some(PersistenceError::Refused { status }) => {
                    assert!(status.contains("tools/store_blocks.py migrate"), "{status}")
                }
                other => panic!("{version}: {other:?}"),
            }
        }
    }
    for change in [
        "CREATE TABLE inode_value (ordinal INTEGER PRIMARY KEY) STRICT",
        "PRAGMA user_version=1",
        "PRAGMA user_version=4",
        "PRAGMA user_version=6",
        "PRAGMA user_version=7",
        "PRAGMA user_version=13",
        "CREATE TABLE metadata_value_group (first_ordinal INTEGER PRIMARY KEY) STRICT",
        "DROP TABLE inode_value_block",
        "DROP TABLE inode_value_block; CREATE TABLE metadata_value_group (first_ordinal INTEGER PRIMARY KEY) STRICT; PRAGMA user_version=1",
    ] {
        let temp = Temp::new("inode-values-old-store");
        let path = temp.join("store.sqlite");
        drop(create(&path));
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(change)
            .unwrap();
        for opened in [
            SqliteHandles::open_writable(config(&path), BINDING, [73; 32]).err(),
            SqliteHandles::open_read_only(config(&path), BINDING, [73; 32]).err(),
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
    assert_eq!(version, 36);
    SqliteHandles::open_writable(config(&path), BINDING, [73; 32]).unwrap();
}

/// The block of four fixed values, byte for byte: the vector
/// `tools/test_store_blocks.py` decodes and encodes to the same bytes, so the
/// diagnostic decoder and the offline migration agree with the engine.
const GOLDEN_VALUES: &str = "010402000081a4000081ed810111111111111111111111111111111111111111111111111111111111111111110080c49fd50c0102030481011111111111111111111111111111111111111111111111111111111111111111010a01020304010222222222222222222222222222222222222222222222222222222222222222223333333333333333333333333333333333333333333333333333333333333333fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff2f3427f";

fn golden_values() -> Vec<InodeValueRow> {
    let inline = |mode: u32, mtime: i64| {
        let mut value = [0u8; 73];
        value[0] = 0x81;
        value[1..9].copy_from_slice(&1u64.to_be_bytes());
        value[9..41].fill(0x11);
        value[41..45].copy_from_slice(&mode.to_be_bytes());
        value[45..53].copy_from_slice(&mtime.to_be_bytes());
        value[53..57].copy_from_slice(&[1, 2, 3, 4]);
        value
    };
    let mut root = [0u8; 73];
    root[0] = 1;
    root[1..9].copy_from_slice(&2u64.to_be_bytes());
    root[9..41].fill(0x22);
    root[41..73].fill(0x33);
    [
        inline(0o100644, 1_700_000_000),
        inline(0o100755, 1_700_000_005),
        root,
        [0xff; 73],
    ]
    .into_iter()
    .zip(5..)
    .map(|(value, ordinal)| InodeValueRow { ordinal, value })
    .collect()
}

#[test]
fn the_block_format_is_the_documented_one() {
    let temp = Temp::new("inode-values-golden");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    publish(&handles, golden_values()).unwrap();
    let (first, last, body): (i64, i64, Vec<u8>) = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT first_ordinal,last_ordinal,body FROM inode_value_block",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!((first, last), (5, 8));
    assert_eq!(body, unhex(GOLDEN_VALUES));
}

fn unhex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}
