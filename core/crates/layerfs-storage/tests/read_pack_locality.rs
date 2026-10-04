//! Count-driven read regression across the existing pack-cache byte boundary.
#[path = "support/memory_metadata.rs"]
mod metadata;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::Storage;
use std::sync::Arc;

#[test]
fn bounded_hash_order_demand_does_not_reacquire_one_pack_for_each_object() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let store = Storage::new(metadata.clone()).unwrap();
    let mut random = 0x8796_aacc_4821_9625u64;
    let mut objects = (0..256)
        .map(|_| {
            let bytes = (0..4096)
                .flat_map(|_| {
                    random ^= random << 13;
                    random ^= random >> 7;
                    random ^= random << 17;
                    random.to_le_bytes()
                })
                .collect::<Vec<_>>();
            FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(&bytes).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let save = store.begin_save().unwrap();
    for object in &objects {
        save.accept(object.clone()).unwrap();
    }
    save.finish().unwrap();
    objects.sort_by_key(FinalizedObject::id);
    let pack_count = metadata.state.lock().unwrap().packs.len();
    assert!(
        pack_count > 8,
        "fixture must cross the2MiB pack-cache bound"
    );
    let reading = Storage::new(metadata.clone()).unwrap();
    let reader = reading.reader().unwrap();
    metadata.calls.lock().unwrap().clear();
    let actual = reader
        .read_objects(&objects.iter().map(FinalizedObject::id).collect::<Vec<_>>())
        .unwrap();
    assert_eq!(
        actual,
        objects
            .iter()
            .map(|o| o.canonical().to_vec())
            .collect::<Vec<_>>()
    );
    let bodies: usize = metadata
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(name, _)| *name == "read_packs" || *name == "read_pack_selection")
        .map(|(_, count)| count)
        .sum();
    // Bounded acquisition consumes each cohort during dependency inspection;
    // reconstruction is the second physical pass.
    eprintln!("locality: {bodies} body acquisitions for {pack_count} packs");
    assert!(
        bodies <= 2 * pack_count,
        "{bodies}body acquisitions for{pack_count}packs"
    );
}

#[test]
fn physical_scheduling_preserves_repeated_ids_and_demand_order() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let store = Storage::new(metadata).unwrap();
    let objects = (0u64..8)
        .map(|n| {
            FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(&n.to_be_bytes()).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let save = store.begin_save().unwrap();
    for object in &objects {
        save.accept(object.clone()).unwrap();
    }
    save.finish().unwrap();
    let wanted = [7, 1, 7, 0, 4, 1, 6];
    let reader = store.reader().unwrap();
    assert_eq!(
        reader
            .read_objects(&wanted.map(|i| objects[i].id()))
            .unwrap(),
        wanted.map(|i| objects[i].canonical().to_vec()).to_vec()
    );
}
