//! Pending exact reuse must retain raw/unplaced C2 eligibility.
mod strict_witness;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::{StorageCapacities, StoragePolicy};
use phase6_live_probe::{
    minio::Minio,
    strict_catalog::{LogicalUse, StrictCatalog},
    strict_writer::Writer,
};
use std::sync::{Arc, Mutex};

#[test]
fn pending_exact_reuse_does_not_seal_or_upload() {
    let directory = std::env::temp_dir().join(format!("sp1-pending-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let catalog = Arc::new(StrictCatalog::create(&directory.join("global.sqlite")).unwrap());
    let provider = Minio {
        authority: "127.0.0.1:1".into(),
        bucket: "uncontacted".into(),
        access: "unused".into(),
        secret: "unused".into(),
        stats: Arc::new(Mutex::new(Default::default())),
    };
    let mut writer = Writer::new(
        catalog.clone(),
        provider.clone(),
        StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap(),
    )
    .unwrap();
    let bytes =
        std::fs::read(strict_witness::evidence().join("old-producer-v1/chunk0.canonical")).unwrap();
    let object = FinalizedObject::new(ObjectRole::Chunk, bytes.clone()).unwrap();
    let id = object.id();
    writer.offer(LogicalUse::RegularFileGraph, object).unwrap();
    writer
        .offer(
            LogicalUse::RegularFileGraph,
            FinalizedObject::new(ObjectRole::Chunk, bytes.clone()).unwrap(),
        )
        .unwrap();
    assert_eq!(writer.counts.exact_reuses, 1);
    assert_eq!(writer.counts.private_group_seals, 0);
    assert_eq!(
        writer.read(LogicalUse::RegularFileGraph, &[id]).unwrap(),
        vec![bytes]
    );
    assert_eq!(provider.statistics().unwrap().put_calls, 0);
    assert_eq!(catalog.capture(Some(writer.save)).unwrap().ceiling, 0);
    catalog.abandon(writer.save).unwrap();
    drop(writer);
    drop(catalog);
    std::fs::remove_dir_all(directory).unwrap();
}
