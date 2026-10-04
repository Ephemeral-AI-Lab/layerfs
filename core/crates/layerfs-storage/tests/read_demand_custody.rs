//! Public-reader regression: a partial cache hit must survive miss admission.
#[path = "support/memory_metadata.rs"]
mod metadata;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::Storage;
use std::sync::Arc;

#[test]
fn mixed_hit_miss_wave_keeps_all_requested_locators_within_existing_bound() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let writer = Storage::new(metadata.clone()).unwrap();
    let objects = (0u64..824)
        .map(|n| {
            FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(&n.to_be_bytes()).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let save = writer.begin_save().unwrap();
    for object in &objects {
        save.accept(object.clone()).unwrap();
    }
    save.finish().unwrap();
    let reader_store = Storage::new(metadata.clone()).unwrap();
    let reader = reader_store.reader().unwrap();
    let first = objects[..512]
        .iter()
        .map(FinalizedObject::id)
        .collect::<Vec<_>>();
    reader.read_objects(&first).unwrap();
    metadata.calls.lock().unwrap().clear();
    let wanted = objects[..200]
        .iter()
        .chain(objects[512..824].iter())
        .collect::<Vec<_>>();
    let ids = wanted.iter().map(|o| o.id()).collect::<Vec<_>>();
    let actual = reader.read_objects(&ids).unwrap();
    assert_eq!(
        actual,
        wanted
            .iter()
            .map(|o| o.canonical().to_vec())
            .collect::<Vec<_>>()
    );
    let calls = metadata.calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .filter(|(name, _)| *name == "locate")
            .copied()
            .collect::<Vec<_>>(),
        vec![("locate", 312)]
    );
}
