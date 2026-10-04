//! Count/cause fixture for locator retention at the existing 4096-entry bound.
#[path = "support/memory_metadata.rs"]
mod metadata;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::{policy, Storage};
use std::sync::Arc;

#[test]
fn pressure_admission_preserves_an_unrequested_positive_locator() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let writer = Storage::new(metadata.clone()).unwrap();
    let mut objects: Vec<_> = (0..policy::READ_OBJECT_LIMIT as u64 + 1)
        .map(|n| {
            FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(&n.to_be_bytes()).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let save = writer.begin_save().unwrap();
    for object in &objects {
        save.accept(object.clone()).unwrap();
    }
    save.finish().unwrap();
    objects.sort_by_key(FinalizedObject::id);
    let reading = Storage::new(metadata.clone()).unwrap();
    let reader = reading.reader().unwrap();
    let ids: Vec<_> = objects[..policy::READ_OBJECT_LIMIT]
        .iter()
        .map(FinalizedObject::id)
        .collect();
    let actual = reader.read_objects(&ids).unwrap();
    assert_eq!(
        actual,
        objects[..policy::READ_OBJECT_LIMIT]
            .iter()
            .map(|o| o.canonical().to_vec())
            .collect::<Vec<_>>()
    );
    metadata.calls.lock().unwrap().clear();
    let miss = &objects[policy::READ_OBJECT_LIMIT];
    assert_eq!(
        reader.read_objects(&[miss.id()]).unwrap(),
        vec![miss.canonical().to_vec()]
    );
    let retained = &objects[policy::READ_OBJECT_LIMIT - 1];
    assert_eq!(
        reader.read_objects(&[retained.id()]).unwrap(),
        vec![retained.canonical().to_vec()]
    );
    let requests: Vec<_> = metadata
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(name, _)| *name == "locate")
        .copied()
        .collect();
    eprintln!("DIAGNOSTIC locator-pressure requests={requests:?}");
    assert_eq!(requests, vec![("locate", 1)]);
    assert_eq!(reading.diagnostics().locator_evictions, 1);
}

#[test]
fn second_chance_breaks_repeated_low_key_victim_churn() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let writer = Storage::new(metadata.clone()).unwrap();
    let mut objects: Vec<_> = (0..policy::READ_OBJECT_LIMIT as u64 + 1)
        .map(|n| {
            FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(&n.to_be_bytes()).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let save = writer.begin_save().unwrap();
    for object in &objects {
        save.accept(object.clone()).unwrap();
    }
    save.finish().unwrap();
    objects.sort_by_key(FinalizedObject::id);
    let reading = Storage::new(metadata.clone()).unwrap();
    let reader = reading.reader().unwrap();
    let ids: Vec<_> = objects[..policy::READ_OBJECT_LIMIT]
        .iter()
        .map(FinalizedObject::id)
        .collect();
    let actual = reader.read_objects(&ids).unwrap();
    assert_eq!(
        actual,
        objects[..policy::READ_OBJECT_LIMIT]
            .iter()
            .map(|o| o.canonical().to_vec())
            .collect::<Vec<_>>()
    );
    metadata.calls.lock().unwrap().clear();
    let miss = &objects[policy::READ_OBJECT_LIMIT];
    assert_eq!(
        reader.read_objects(&[miss.id()]).unwrap(),
        vec![miss.canonical().to_vec()]
    );
    let retained = &objects[0];
    assert_eq!(
        reader.read_objects(&[retained.id()]).unwrap(),
        vec![retained.canonical().to_vec()]
    );
    for _ in 0..16 {
        for object in [&objects[1], &objects[0]] {
            assert_eq!(
                reader.read_objects(&[object.id()]).unwrap(),
                vec![object.canonical().to_vec()]
            );
        }
    }
    let requests: Vec<_> = metadata
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(name, _)| *name == "locate")
        .copied()
        .collect();
    eprintln!("DIAGNOSTIC locator-pressure requests={requests:?}");
    assert_eq!(requests, vec![("locate", 1); 3]);
    assert_eq!(reading.diagnostics().locator_evictions, 3);
    assert!(
        reading.diagnostics().locator_eviction_probes <= 2 * policy::READ_OBJECT_LIMIT as u64 + 3
    );
    assert!(
        reading.diagnostics().locator_bookkeeping_live_peak_bytes
            <= policy::READ_OBJECT_LIMIT as u64 * 8 + 64
    );
}
