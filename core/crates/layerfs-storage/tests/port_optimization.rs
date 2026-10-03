//! Work-count regression checks through ordinary public save/read APIs.
#[path = "support/memory_engines.rs"]
mod engines;
mod support;
use engines::{MemoryMetadata, MemoryObjects};
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::{policy::WAVE_CANONICAL_BYTES_LIMIT, Storage};
use std::sync::Arc;
#[test]
fn reservation_blocks_cover_later_waves_and_finish_without_reallocation() {
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    let storage = Storage::new(metadata.clone(), objects.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    let mut ids = Vec::new();
    for i in 0..1600_u32 {
        let mut raw = vec![3; 600];
        raw[..4].copy_from_slice(&i.to_be_bytes());
        let object =
            FinalizedObject::new(ObjectRole::WholeFile, support::assembled_small_object(&raw))
                .unwrap();
        ids.push((object.id(), object.canonical().to_vec()));
        save.accept(object).unwrap();
        assert!(save.pending_canonical_bytes() <= WAVE_CANONICAL_BYTES_LIMIT);
    }
    let outcome = save.finish().unwrap();
    assert_eq!(outcome.inserted, 1600);
    let calls = metadata.calls.lock().unwrap();
    let reserves = calls.iter().filter(|(name, _)| *name == "reserve").count();
    assert_eq!(
        reserves, 1,
        "unused IDs must cover later waves and final seals"
    );
    drop(calls);
    let reopened = Storage::new(metadata, objects).unwrap();
    for (id, bytes) in ids.iter().step_by(64) {
        assert_eq!(
            reopened.reader().unwrap().read_objects(&[*id]).unwrap(),
            vec![bytes.clone()]
        );
    }
}

#[test]
fn mixed_file_lanes_share_budget_and_reopen_exact_bytes_with_fewer_payload_packs() {
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    let storage = Storage::new(metadata.clone(), objects.clone()).unwrap();
    let dir = support::TempDir::new("mixed-lanes");
    let path = dir.store_path("old");
    let old = support::create_store(&path);
    let operation = storage.begin_save().unwrap();
    let mut expected = Vec::new();
    let mut legacy = Vec::new();
    for i in 0..20_u32 {
        let mut raw = support::noise(if i % 2 == 0 { 96000 } else { 200000 });
        raw[..4].copy_from_slice(&i.to_be_bytes());
        let (bag, root, _) = support::construct_file(&raw);
        for object in bag.finalized() {
            operation.accept(object.clone()).unwrap();
            legacy.push(object);
        }
        assert!(operation.pending_canonical_bytes() <= WAVE_CANONICAL_BYTES_LIMIT);
        expected.push((root, raw));
    }
    operation.finish().unwrap();
    support::disabled(|scope| {
        let mut save = old.begin_save(scope.child("save"))?;
        for object in legacy {
            save.accept(object)?;
        }
        save.finish(scope.child("finish"))
    })
    .unwrap();
    let (_, payloads) = engines::snapshot(&path);
    let actual = objects.bodies.lock().unwrap().len();
    assert!(
        actual < payloads.len(),
        "mixed lane flushes: current {actual}, reference {}",
        payloads.len()
    );
    let reopened = Storage::new(metadata, objects).unwrap();
    for (root, raw) in expected {
        let mut bytes = Vec::new();
        support::disabled(|scope| {
            layerfs_content::read_all(
                &reopened.reader().unwrap(),
                root,
                &mut bytes,
                scope.child("read"),
            )
        })
        .unwrap();
        assert_eq!(bytes, raw);
    }
}
