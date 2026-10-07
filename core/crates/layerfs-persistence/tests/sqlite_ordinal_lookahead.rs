//! Combined Save reservations preserve value reuse and acknowledged gaps.
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
        PersistenceConfig::sqlite(path)
            .with_sqlite_profile(layerfs_persistence::SqlitePersistenceProfile::Disposable),
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
fn fresh_leaves_share_one_combined_reservation_and_deduplicate_values() {
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
    assert_eq!(storage.diagnostics().ordinal_reservations, before + 1);
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

#[test]
fn small_configured_blocks_refill_without_limiting_the_save() {
    let t = support::Temp::new("counted-refills");
    let h = create(&t.join("db"));
    let storage = Storage::with_reservations(
        h.storage.clone(),
        layerfs_storage::ReservationBlocks {
            packs: 1,
            ordinals: 3,
        },
    )
    .unwrap();
    let objects = (0..5)
        .map(|n| leaf(1 + n * 2, &[1 + n * 2, 2 + n * 2]))
        .collect::<Vec<_>>();
    let before = h.diagnostics().unwrap();
    let save = storage.begin_save().unwrap();
    assert_eq!(
        h.diagnostics().unwrap().write_commits - before.write_commits,
        1
    );
    for object in &objects {
        save.accept(object.clone()).unwrap();
    }
    let result = save.finish().unwrap();
    assert_eq!(result.pool.new_values, 10);
    read(&storage, &objects);
    let counts = storage.diagnostics();
    assert_eq!(counts.initial_reservations, 1);
    assert_eq!(counts.ordinal_reservations, 5);
    assert!(
        counts.reservation_refills > 4,
        "both ranges cross their configured windows"
    );
    assert_eq!(
        counts.reserve,
        counts.initial_reservations + counts.reservation_refills
    );
    assert_eq!(
        h.diagnostics().unwrap().write_commits - before.write_commits,
        counts.reserve + counts.publish
    );
    println!("SAVE_BLOCK_REFILLS initial={} refill={} ordinal={} publication={} write_transactions={} saved_values=10 reconstructed_leaves=5", counts.initial_reservations, counts.reservation_refills, counts.ordinal_reservations, counts.publish, counts.reserve + counts.publish);
}

#[path = "../../layerfs-daemon/tests/support/held_writer.rs"]
mod held_writer;

#[test]
fn initial_combined_reservation_is_busy_before_effect() {
    let t = support::Temp::new("combined-busy");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let held = held_writer::HeldWriter::acquire(&t.join("db"));
    let before = h.diagnostics().unwrap();
    assert!(matches!(
        storage.begin_save(),
        Err(layerfs_storage::StorageError::Busy)
    ));
    assert_eq!(
        h.diagnostics().unwrap().write_transactions,
        before.write_transactions
    );
    assert_eq!(storage.diagnostics().initial_reservations, 1);
    held.release();
    let save = storage.begin_save().unwrap();
    let object = leaf(1, &[5]);
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    read(&storage, &[object]);
    let sql = rusqlite::Connection::open(t.join("db")).unwrap();
    let first: i64 = sql
        .query_row("SELECT min(pack_id) FROM pack", [], |row| row.get(0))
        .unwrap();
    assert_eq!(first, 1, "Busy consumed no pack ids");
    let first: i64 = sql
        .query_row(
            "SELECT min(first_ordinal) FROM metadata_value_group",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(first, 1, "Busy consumed no value ordinals");
    println!("SAVE_BLOCK_BUSY initial_attempts=2 busy_effect=0 later_explicit_save=complete first_pack=1 first_ordinal=1 child=released-and-joined");
}
