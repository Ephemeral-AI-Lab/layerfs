//! Save must consume bounded physical input before unrelated discovery displaces it.
#[path = "support/memory_metadata.rs"]
mod metadata;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::Storage;
use std::sync::Arc;

#[test]
fn exact_reuse_does_not_scan_the_wave_before_consuming_its_objects() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let store = Storage::new(metadata.clone()).unwrap();
    let mut random = 0x8796_aacc_4821_9625u64;
    let objects = (0..256)
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
    let pack_count = metadata.state.lock().unwrap().packs.len();
    assert!(pack_count > 8, "fixture must exceed the2MiB owner cache");
    let reading = Storage::new(metadata.clone()).unwrap();
    metadata.acquired_pack_ids.lock().unwrap().clear();
    let save = reading.begin_save().unwrap();
    for object in &objects {
        save.accept(object.clone()).unwrap();
    }
    let outcome = save.finish().unwrap();
    assert_eq!(outcome.reused, objects.len() as u64);
    assert_eq!(outcome.inserted, 0);
    let reads = metadata.acquired_pack_ids.lock().unwrap();
    eprintln!(
        "Save exact reuse: {} acquisitions, {pack_count} distinct packs",
        reads.len()
    );
    // The unchanged sparse-first/metadata promotion policy may acquire each
    // pack twice: its first group, then the whole pack on a sibling demand.
    assert!(
        reads.len() <= 2 * pack_count,
        "wave discovery displaced input before consumption: {} reads for {pack_count} packs",
        reads.len()
    );
}
