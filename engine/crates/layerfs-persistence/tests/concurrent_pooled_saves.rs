//! Reservation order is independent of publication order across public Saves.
mod support;
use layerfs_content::{
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
    },
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{Storage, StoragePolicy};

fn leaf(serial: u64, value: u64) -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::InodeLeaf,
        InodeLeaf {
            subtree_bytes: LEAF_ROW_BYTES as u64,
            rows: vec![InodeLeafRow {
                serial,
                value: encode_inode_value(InodeValue {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: 1,
                    content_root: ObjectId::for_bytes(&value.to_le_bytes()),
                    metadata_root: layerfs_content::object::inode_leaf::InodeMetadata::Root(
                        ObjectId::for_bytes(b"concurrent-pooled-metadata"),
                    ),
                }),
            }],
        }
        .encode()
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn earlier_reserved_save_finishes_after_a_higher_block_was_published() {
    let temp = support::Temp::new("concurrent-pooled-saves");
    let handles = Handles::create(
        PersistenceConfig::sqlite(temp.join("store.sqlite"))
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"concurrent-pooled-saves".to_vec(),
            cursor_key: [43; 32],
            incarnation: 1,
        },
    )
    .unwrap();
    let earlier = Storage::new(handles.storage.clone()).unwrap();
    let later = Storage::new(handles.storage.clone()).unwrap();
    // Both actual initial reservations complete before either pooled group is
    // constructed. No thread, fake persistence port, or scheduling assumption.
    let first = earlier.begin_save().unwrap();
    let second = later.begin_save().unwrap();
    let a = leaf(1, 101);
    let b = leaf(2, 202);
    second.accept(b.clone()).unwrap();
    second.finish().unwrap();
    first.accept(a.clone()).unwrap();
    let saved = first
        .finish()
        .expect("the original disjoint reserved block remains valid");
    assert_eq!(saved.pool.new_values, 1);
    assert_eq!(earlier.diagnostics().initial_reservations, 1);
    assert_eq!(later.diagnostics().initial_reservations, 1);
    assert_eq!(earlier.diagnostics().reservation_refills, 0);
    let fresh = Storage::new(handles.storage.clone()).unwrap();
    let reader = fresh.reader().unwrap();
    assert_eq!(
        reader.read_objects(&[a.id(), b.id()]).unwrap(),
        vec![a.canonical().to_vec(), b.canonical().to_vec()]
    );
    let duplicate_a = leaf(3, 101);
    let duplicate_b = leaf(4, 202);
    let next = fresh.begin_save().unwrap();
    next.accept(duplicate_a.clone()).unwrap();
    next.accept(duplicate_b.clone()).unwrap();
    let outcome = next.finish().unwrap();
    // Neither leaf names a stored leaf, so this Save cannot know that both
    // values already have a row: each is stored once more.
    assert_eq!(outcome.pool.new_values, 2);
    assert_eq!(outcome.pool.reused_values, 0);
    assert_eq!(
        reader
            .read_objects(&[duplicate_a.id(), duplicate_b.id()])
            .unwrap(),
        vec![
            duplicate_a.canonical().to_vec(),
            duplicate_b.canonical().to_vec()
        ]
    );
}
