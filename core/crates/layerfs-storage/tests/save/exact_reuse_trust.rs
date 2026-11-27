//! Exact reuse trusts the content address (owner, 2026-10-19; #352 record
//! 034 A-1).
//!
//! A Save that offers an object the Store already holds, under the same role
//! and canonical length, accepts it without reading the stored copy back. A
//! role or length mismatch is still refused at Save. A stored copy that was
//! corrupted after it was written is therefore accepted by the reusing Save,
//! and its next read fails authentication against the id.
use crate::metadata;
use crate::support;

use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeMetadata, InodeValue,
    LEAF_ROW_BYTES,
};
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::port::ObjectKey;
use layerfs_storage::{Storage, StorageError, StoragePolicy};
use metadata::MemoryMetadata;
use std::sync::Arc;

fn object() -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::WholeFile,
        support::assembled_small_object(&support::noise(8 * 1024)),
    )
    .unwrap()
}

/// A Store holding `object()`, written by its own Save.
fn stored() -> Arc<MemoryMetadata> {
    let metadata = Arc::new(MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = StoragePolicy::default();
    let storage = Storage::new(metadata.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object()).unwrap();
    assert_eq!(save.finish().unwrap().inserted, 1);
    metadata
}

/// Pack-body reads the metadata port served since the last clear.
fn pack_reads(metadata: &MemoryMetadata) -> usize {
    metadata
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(call, _)| call.starts_with("read_"))
        .count()
}

/// Saves `object()` again on a fresh handle (nothing cached).
fn save_again(
    metadata: &Arc<MemoryMetadata>,
) -> Result<layerfs_storage::WriteOutcome, StorageError> {
    let storage = Storage::new(metadata.clone()).unwrap();
    metadata.calls.lock().unwrap().clear();
    let save = storage.begin_save().unwrap();
    save.accept(object()).unwrap();
    save.finish()
}

#[test]
fn an_exact_reuse_reads_no_stored_bytes() {
    let metadata = stored();
    let outcome = save_again(&metadata).unwrap();
    assert_eq!((outcome.inserted, outcome.reused), (0, 1));
    assert_eq!(pack_reads(&metadata), 0, "the stored copy is not read back");
}

#[test]
fn a_role_or_length_mismatch_is_still_refused_at_save() {
    let id = object().id();
    for tamper in [
        |row: &mut layerfs_storage::location::ObjectLocation| row.role = ObjectRole::Chunk,
        |row: &mut layerfs_storage::location::ObjectLocation| row.canonical_length += 1,
    ] {
        let metadata = stored();
        tamper(metadata.state.lock().unwrap().objects.get_mut(&id).unwrap());
        let error = save_again(&metadata).unwrap_err();
        assert!(
            matches!(error, StorageError::Collision(collided) if collided == id),
            "{error:?}"
        );
    }
}

#[test]
fn a_corrupt_stored_copy_is_accepted_at_save_and_fails_at_its_next_read() {
    let metadata = stored();
    let id = object().id();
    // Flip one payload byte inside the stored record and re-seal the pack's
    // digest, so the damage is only visible to object authentication.
    {
        let mut state = metadata.state.lock().unwrap();
        let pack_id = state.objects[&id].pack_id;
        let pack = state.packs.get_mut(&pack_id).unwrap();
        let raw = support::noise(8 * 1024);
        let mut body = pack.body.as_ref().clone();
        let at = body
            .windows(64)
            .position(|window| window == &raw[4_096..4_160])
            .expect("the record stores its payload verbatim");
        body[at] ^= 0x01;
        pack.info.key = ObjectKey::for_bytes(&body);
        pack.body = Arc::new(body);
    }

    let outcome = save_again(&metadata).unwrap();
    assert_eq!((outcome.inserted, outcome.reused), (0, 1));
    assert_eq!(pack_reads(&metadata), 0);

    let storage = Storage::new(metadata.clone()).unwrap();
    let error = storage.reader().unwrap().read_objects(&[id]).unwrap_err();
    assert!(
        matches!(error, StorageError::Integrity("dependency identity")),
        "{error:?}"
    );
}

/// A pooled metadata leaf reused by a later Save acquires no pack either:
/// its values are not rebuilt.
#[test]
fn an_exact_reuse_of_a_pooled_leaf_acquires_no_pack() {
    let metadata = Arc::new(MemoryMetadata::default());
    let rows: Vec<InodeLeafRow> = (1u64..=64)
        .map(|serial| InodeLeafRow {
            serial,
            value: encode_inode_value(InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: ObjectId::for_bytes(&serial.to_be_bytes()),
                metadata_root: InodeMetadata::Root(ObjectId::for_bytes(b"metadata")),
            }),
        })
        .collect();
    let canonical = InodeLeaf {
        subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
        rows,
    }
    .encode()
    .unwrap();
    let leaf = FinalizedObject::new(ObjectRole::InodeLeaf, canonical).unwrap();
    let save = Storage::new(metadata.clone()).unwrap();
    let first = save.begin_save().unwrap();
    first.accept(leaf.clone()).unwrap();
    first.finish().unwrap();

    let storage = Storage::new(metadata.clone()).unwrap();
    metadata.calls.lock().unwrap().clear();
    metadata.acquired_pack_ids.lock().unwrap().clear();
    let again = storage.begin_save().unwrap();
    again.accept(leaf.clone()).unwrap();
    let outcome = again.finish().unwrap();
    assert_eq!((outcome.inserted, outcome.reused), (0, 1));
    assert_eq!(pack_reads(&metadata), 0);
    assert!(metadata.acquired_pack_ids.lock().unwrap().is_empty());
    // The stored leaf still reads back whole.
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[leaf.id()])
            .unwrap(),
        vec![leaf.canonical().to_vec()]
    );
}
