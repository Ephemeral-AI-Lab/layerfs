//! Port-path reads of unchanged sealed bytes, authenticated through C2.
#[path = "support/memory_engines.rs"]
mod engines;
mod support;

use engines::{MemoryMetadata, MemoryObjects};
use layerfs_content::{
    AdvisoryPredecessors, AuthenticatedObjects, FinalizedObject, ObjectId, ObjectRole,
    PredecessorProvenance,
};
use layerfs_storage::{
    port::{MetadataError, MetadataStore, ObjectError, ObjectStore},
    Storage, StorageError,
};
use std::sync::Arc;
use support::{assembled_small_object, create_store, noise, patterned, save_one, TempDir};

fn install(path: &std::path::Path) -> (Arc<MemoryMetadata>, Arc<MemoryObjects>) {
    let (batch, payload) = engines::snapshot(path);
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    for (key, body) in payload {
        objects.put_if_absent(key, &body).unwrap();
    }
    metadata.register(&batch).unwrap();
    (metadata, objects)
}
fn whole(raw: &[u8]) -> FinalizedObject {
    FinalizedObject::new(ObjectRole::WholeFile, assembled_small_object(raw)).unwrap()
}
type PrefixFixture = (
    TempDir,
    ObjectId,
    Vec<u8>,
    Arc<MemoryMetadata>,
    Arc<MemoryObjects>,
);

fn prefix_fixture() -> PrefixFixture {
    let dir = TempDir::new("port-prefix");
    let path = dir.store_path("store");
    let store = create_store(&path);
    let raw = patterned(100_000);
    let base = whole(&raw);
    let base_id = base.id();
    save_one(&store, base).unwrap();
    let mut changed = raw;
    changed[40_000..41_000].copy_from_slice(&noise(1_000));
    let mut predecessors = AdvisoryPredecessors::new();
    predecessors
        .push(base_id, PredecessorProvenance::OriginalBase)
        .unwrap();
    let dependent = whole(&changed).with_predecessors(predecessors);
    let id = dependent.id();
    assert_eq!(save_one(&store, dependent).unwrap().prefix_records, 1);
    let (metadata, objects) = install(&path);
    (dir, id, assembled_small_object(&changed), metadata, objects)
}
#[test]
fn port_read_authenticates_prefix_across_packs_and_batches_a_read_wave() {
    let (_dir, id, expected, metadata, objects) = prefix_fixture();
    let storage = Storage::new(metadata.clone(), objects.clone()).unwrap();
    let reader = storage.reader().unwrap();
    assert_eq!(
        reader.read_canonical_batch(&[id, id]).unwrap(),
        vec![expected.clone(), expected]
    );
    let counters = storage.diagnostics();
    assert_eq!(counters.policy, 1);
    assert_eq!(counters.locate, 2);
    assert_eq!(counters.gets, 2);
    assert!(counters.locate <= 6);
    assert!(counters.locator_hits > 0);
    assert!(counters.pack_hits > 0);
    assert_eq!(
        metadata
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(op, _)| *op == "locate")
            .count() as u64,
        counters.locate
    );
    println!("DIAGNOSTIC prefix-read {counters:?}");
}
#[test]
fn wrong_pack_digest_and_missing_payload_are_refused_once() {
    let (_dir, id, _expected, metadata, objects) = prefix_fixture();
    let key = metadata
        .state
        .lock()
        .unwrap()
        .packs
        .values()
        .find(|p| p.info.domain == layerfs_storage::location::PackDomain::Payload)
        .unwrap()
        .info
        .key;
    objects.bodies.lock().unwrap().get_mut(&key).unwrap()[24] ^= 1;
    let storage = Storage::new(metadata.clone(), objects.clone()).unwrap();
    assert!(matches!(
        storage.reader().unwrap().read_objects(&[id]),
        Err(StorageError::Integrity("sealed pack length/digest"))
    ));
    assert!(storage.diagnostics().gets <= 2);
    objects.bodies.lock().unwrap().remove(&key);
    let storage = Storage::new(metadata, objects).unwrap();
    assert!(matches!(
        storage.reader().unwrap().read_objects(&[id]),
        Err(StorageError::Io(error)) if error.get_ref().and_then(|error| error.downcast_ref::<ObjectError>()) == Some(&ObjectError::Missing)
    ));
    assert!(storage.diagnostics().gets <= 2);
}
#[test]
fn missing_base_is_an_explicit_refusal_and_missing_root_stays_distinct() {
    let (_dir, id, _expected, metadata, objects) = prefix_fixture();
    metadata
        .state
        .lock()
        .unwrap()
        .objects
        .retain(|key, _| *key == id);
    let storage = Storage::new(metadata, objects).unwrap();
    let reader = storage.reader().unwrap();
    assert!(matches!(
        reader.read_objects(&[id]),
        Err(StorageError::ObjectMissing(_))
    ));
    assert!(matches!(
        reader.read_objects(&[ObjectId::for_bytes(b"absent")]),
        Err(StorageError::ObjectMissing(_))
    ));
    assert_eq!(storage.diagnostics().locate, 3);
}
#[test]
fn stored_canonical_identity_is_checked_after_pack_digest() {
    let (_dir, id, _expected, metadata, objects) = prefix_fixture();
    let forged = ObjectId::for_bytes(b"forged canonical id");
    let mut state = metadata.state.lock().unwrap();
    let mut row = state.objects.remove(&id).unwrap();
    row.object_id = forged;
    state.objects.insert(forged, row);
    drop(state);
    let storage = Storage::new(metadata, objects).unwrap();
    assert!(storage.reader().unwrap().read_objects(&[forged]).is_err());
}
#[test]
fn pooled_leaves_are_registered_then_reconstructed_over_metadata_port() {
    use layerfs_content::inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
    };
    let dir = TempDir::new("port-pooled");
    let path = dir.store_path("store");
    let store = create_store(&path);
    let rows = (1u64..=100)
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
    let leaf = FinalizedObject::new(ObjectRole::InodeLeaf, canonical.clone()).unwrap();
    let id = leaf.id();
    save_one(&store, leaf).unwrap();
    let (metadata, objects) = install(&path);
    let storage = Storage::new(metadata, objects).unwrap();
    assert_eq!(
        storage.reader().unwrap().read_objects(&[id, id]).unwrap(),
        vec![canonical.clone(), canonical]
    );
    let counters = storage.diagnostics();
    assert_eq!(counters.gets, 0);
    assert_eq!(counters.locate, 1);
    assert_eq!(counters.value_groups, 1);
    assert!(counters.read_packs <= 4);
    println!("DIAGNOSTIC pooled-read {counters:?}");
}
#[test]
fn read_bound_is_refused_before_a_port_call_and_empty_demand_is_empty() {
    let storage = Storage::new(
        Arc::new(MemoryMetadata::default()),
        Arc::new(MemoryObjects::default()),
    )
    .unwrap();
    let reader = storage.reader().unwrap();
    assert!(reader.read_objects(&[]).unwrap().is_empty());
    let ids = vec![ObjectId::for_bytes(b"x"); layerfs_storage::policy::READ_OBJECT_LIMIT + 1];
    assert!(matches!(
        reader.read_objects(&ids),
        Err(StorageError::CapacityExceeded { .. })
    ));
    assert_eq!(storage.diagnostics().locate, 0);
}
#[test]
fn both_uncertain_port_errors_keep_unknown_outcome() {
    assert!(StorageError::from(ObjectError::Uncertain).is_unknown_outcome());
    assert!(StorageError::from(MetadataError::Uncertain).is_unknown_outcome());
    for error in [
        ObjectError::Missing,
        ObjectError::Malformed,
        ObjectError::Refused { status: 403 },
    ] {
        assert!(!StorageError::from(error).is_unknown_outcome());
    }
    for error in [
        MetadataError::Missing,
        MetadataError::Malformed,
        MetadataError::Refused {
            status: "23505".into(),
        },
    ] {
        assert!(!StorageError::from(error).is_unknown_outcome());
    }
}
