//! Exact fresh-leaf lookahead preserves dense ordinals, reuse and acknowledged gaps.
mod support;
use layerfs_content::{
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
    },
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig};
use layerfs_storage::{Storage, StoragePolicy};
fn create(path: &std::path::Path) -> Handles {
    Handles::create(
        PersistenceConfig::sqlite(path),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"ordinal-lookahead".to_vec(),
            incarnation: 1,
            cursor_key: [71; 32],
        },
    )
    .unwrap()
}
fn leaf(serial: u64, values: &[u64]) -> FinalizedObject {
    let rows = values
        .iter()
        .enumerate()
        .map(|(offset, value)| InodeLeafRow {
            serial: serial + offset as u64,
            value: encode_inode_value(InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: ObjectId::for_bytes(&value.to_le_bytes()),
                metadata_root: ObjectId::for_bytes(b"metadata"),
            }),
        })
        .collect::<Vec<_>>();
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
fn read(storage: &Storage, objects: &[FinalizedObject]) {
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&objects.iter().map(FinalizedObject::id).collect::<Vec<_>>())
            .unwrap(),
        objects
            .iter()
            .map(|object| object.canonical().to_vec())
            .collect::<Vec<_>>()
    );
}
#[test]
fn four_fresh_leaves_share_one_exact_reservation_and_deduplicate_values() {
    let t = support::Temp::new("ordinal-exact");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let objects = vec![
        leaf(1, &[1, 2]),
        leaf(3, &[2, 3]),
        leaf(5, &[3, 4]),
        leaf(7, &[4, 5]),
    ];
    let save = storage.begin_save().unwrap();
    for object in &objects {
        save.accept(object.clone()).unwrap();
    }
    let result = save.finish().unwrap();
    assert_eq!(result.pool.new_values, 5);
    assert_eq!(result.pool.reused_values, 3);
    assert_eq!(storage.diagnostics().ordinal_reservations, 1);
    read(&storage, &objects);
    let sql = rusqlite::Connection::open(t.join("db")).unwrap();
    let allocator: i64 = sql
        .query_row("SELECT next_ordinal FROM store_policy", [], |r| r.get(0))
        .unwrap();
    assert_eq!(allocator, 6);
    let groups = sql
        .prepare("SELECT first_ordinal,count FROM metadata_value_group ORDER BY first_ordinal")
        .unwrap()
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(groups, vec![(1, 2), (3, 1), (4, 1), (5, 1)]);
    let before = storage.diagnostics().ordinal_reservations;
    let duplicate = leaf(20, &[1, 2, 3, 4, 5]);
    let save = storage.begin_save().unwrap();
    save.accept(duplicate.clone()).unwrap();
    assert_eq!(save.finish().unwrap().pool.new_values, 0);
    assert_eq!(storage.diagnostics().ordinal_reservations, before);
    read(&storage, &[duplicate]);
}
#[test]
fn a_foreign_acknowledged_reservation_is_never_recycled_by_the_plan() {
    use layerfs_storage::port::{PackPersistence, Reserve};
    let t = support::Temp::new("ordinal-gap");
    let h = create(&t.join("db"));
    assert_eq!(
        h.storage
            .reserve(Reserve {
                packs: 0,
                ordinals: 7
            })
            .unwrap()
            .first_ordinal,
        1
    );
    let storage = Storage::new(h.storage.clone()).unwrap();
    let objects = vec![leaf(1, &[9]), leaf(2, &[10])];
    let save = storage.begin_save().unwrap();
    for object in &objects {
        save.accept(object.clone()).unwrap();
    }
    save.finish().unwrap();
    read(&storage, &objects);
    let sql = rusqlite::Connection::open(t.join("db")).unwrap();
    let first: i64 = sql
        .query_row(
            "SELECT min(first_ordinal) FROM metadata_value_group",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(first, 8);
    let next: i64 = sql
        .query_row("SELECT next_ordinal FROM store_policy", [], |r| r.get(0))
        .unwrap();
    assert_eq!(next, 10);
}
