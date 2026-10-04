//! A pooled demand must rebuild its physical leaf only once before authentication.
#[path = "support/memory_metadata.rs"]
mod metadata;
use layerfs_content::{
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
    },
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_storage::Storage;
use std::sync::Arc;

#[test]
fn pooled_demand_reuses_one_checked_physical_reconstruction() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let store = Storage::new(metadata.clone()).unwrap();
    let rows = (1..=64)
        .map(|serial| InodeLeafRow {
            serial,
            value: encode_inode_value(InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: ObjectId::for_bytes(&serial.to_be_bytes()),
                metadata_root: ObjectId::for_bytes(b"metadata"),
            }),
        })
        .collect::<Vec<_>>();
    let canonical = InodeLeaf {
        subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
        rows,
    }
    .encode()
    .unwrap();
    let object = FinalizedObject::new(ObjectRole::InodeLeaf, canonical.clone()).unwrap();
    let save = store.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    let reading = Storage::new(metadata.clone()).unwrap();
    let reader = reading.reader().unwrap();
    metadata.calls.lock().unwrap().clear();
    assert_eq!(
        reader.read_objects(&[object.id()]).unwrap(),
        vec![canonical.clone()]
    );
    let work = reader.pooled_read_counters();
    assert_eq!(work.leaf_requests, 1);
    // The unchanged chain algorithm extracts FULL once while walking and once
    // while decoding. An ordinal-prefetch prepass must not double that work.
    assert_eq!(work.physical_record_calls, 2);
    assert_eq!(work.chain_edges, 0);
    assert_eq!(work.physical_group_decodes, 1);
    assert_eq!(
        metadata
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| *name == "value_groups")
            .count(),
        1
    );
    assert_eq!(
        reader.read_objects(&[object.id()]).unwrap(),
        vec![canonical]
    );
    let next = reader.pooled_read_counters();
    assert_eq!(next.leaf_requests, 2);
    assert_eq!(next.physical_record_calls, 4);
    assert_eq!(next.physical_group_decodes, 1);
}

#[test]
fn missing_catalogue_still_refuses_canonical_reconstruction() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let store = Storage::new(metadata.clone()).unwrap();
    let canonical = InodeLeaf {
        subtree_bytes: LEAF_ROW_BYTES as u64,
        rows: vec![InodeLeafRow {
            serial: 1,
            value: encode_inode_value(InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: ObjectId::for_bytes(b"content"),
                metadata_root: ObjectId::for_bytes(b"metadata"),
            }),
        }],
    }
    .encode()
    .unwrap();
    let object = FinalizedObject::new(ObjectRole::InodeLeaf, canonical).unwrap();
    let save = store.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    metadata.state.lock().unwrap().groups.clear();
    let reading = Storage::new(metadata).unwrap();
    assert!(reading
        .reader()
        .unwrap()
        .read_objects(&[object.id()])
        .is_err());
}

#[test]
fn save_exact_reuse_consumes_the_group_already_acquired_by_discovery() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let store = Storage::new(metadata.clone()).unwrap();
    let canonical = InodeLeaf {
        subtree_bytes: 64 * LEAF_ROW_BYTES as u64,
        rows: (1u64..=64)
            .map(|serial| InodeLeafRow {
                serial,
                value: encode_inode_value(InodeValue {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: 1,
                    content_root: ObjectId::for_bytes(&serial.to_be_bytes()),
                    metadata_root: ObjectId::for_bytes(b"metadata"),
                }),
            })
            .collect(),
    }
    .encode()
    .unwrap();
    let object = FinalizedObject::new(ObjectRole::InodeLeaf, canonical).unwrap();
    let save = store.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    let pack = metadata.state.lock().unwrap().objects[&object.id()].pack_id;
    let reading = Storage::new(metadata.clone()).unwrap();
    metadata.acquired_pack_ids.lock().unwrap().clear();
    let save = reading.begin_save().unwrap();
    save.accept(object).unwrap();
    let outcome = save.finish().unwrap();
    assert_eq!(outcome.reused, 1);
    assert_eq!(outcome.inserted, 0);
    let reads = metadata.acquired_pack_ids.lock().unwrap();
    assert_eq!(
        reads.iter().filter(|id| **id == pack).count(),
        1,
        "discovery and reconstruction acquired the same immutable ordinary pack: {reads:?}"
    );
}
