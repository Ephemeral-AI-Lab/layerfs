//! Logical membership does not require reconstructing unrelated child payloads.
mod support;
use layerfs_content::{
    file::mapping::{encode_chunk_object, encode_node, ExtentNode, ExtentSlice},
    FinalizedObject, ObjectRole,
};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig};
use layerfs_storage::{Storage, StorageError, StoragePolicy};
fn create(path: &std::path::Path) -> Handles {
    Handles::create(
        PersistenceConfig::sqlite(path),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"logical-reference-acquisition".to_vec(),
            cursor_key: [71; 32],
            incarnation: 1,
        },
    )
    .unwrap()
}
fn chunk(seed: u64) -> FinalizedObject {
    let mut state = seed;
    let raw = (0..32768)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect::<Vec<_>>();
    FinalizedObject::new(ObjectRole::Chunk, encode_chunk_object(&raw).unwrap()).unwrap()
}
fn leaf(chunks: &[FinalizedObject]) -> FinalizedObject {
    let refs = chunks.iter().map(FinalizedObject::id).collect::<Vec<_>>();
    let node = ExtentNode::Leaf {
        subtree_logical_bytes: chunks.len() as u64 * 32768,
        extents: refs
            .iter()
            .map(|id| ExtentSlice::new(*id, 0, 32768).unwrap())
            .collect(),
    };
    FinalizedObject::new(ObjectRole::ExtentLeaf, encode_node(&node, true).unwrap())
        .unwrap()
        .with_references(refs)
}
#[test]
fn new_extent_leaf_checks_child_membership_without_reading_child_packs() {
    let t = support::Temp::new("logical-membership");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let chunks = (1..=128).map(chunk).collect::<Vec<_>>();
    let save = storage.begin_save().unwrap();
    for o in &chunks {
        save.accept(o.clone()).unwrap();
    }
    save.finish().unwrap();
    let parent = leaf(&chunks);
    let before = storage.diagnostics();
    let save = storage.begin_save().unwrap();
    save.accept(parent.clone()).unwrap();
    assert_eq!(save.finish().unwrap().inserted, 1);
    let after = storage.diagnostics();
    eprintln!(
        "DIAGNOSTIC logical child acquisition: batched={} individual={} bytes={}",
        after.read_packs - before.read_packs,
        after.payload_reads - before.payload_reads,
        after.pack_read_bytes + after.payload_read_bytes
            - before.pack_read_bytes
            - before.payload_read_bytes
    );
    assert_eq!(after.read_packs, before.read_packs);
    assert_eq!(after.payload_reads, before.payload_reads);
    // The leaf and every referenced payload remain authenticated and readable.
    let mut objects = chunks;
    objects.push(parent);
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&objects.iter().map(FinalizedObject::id).collect::<Vec<_>>())
            .unwrap(),
        objects
            .iter()
            .map(|o| o.canonical().to_vec())
            .collect::<Vec<_>>()
    );
}
#[test]
fn missing_logical_child_is_still_refused() {
    let t = support::Temp::new("missing-logical-child");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let child = chunk(991);
    let parent = leaf(std::slice::from_ref(&child));
    let save = storage.begin_save().unwrap();
    save.accept(parent.clone()).unwrap();
    assert!(
        matches!(save.finish(),Err(StorageError::MissingDependency {object,reference}) if object==parent.id() && reference==child.id())
    );
}
#[test]
fn exact_reuse_still_acquires_and_compares_canonical_bytes() {
    let t = support::Temp::new("exact-reuse-acquisition");
    let h = create(&t.join("db"));
    let storage = Storage::new(h.storage.clone()).unwrap();
    let o = chunk(741);
    let save = storage.begin_save().unwrap();
    save.accept(o.clone()).unwrap();
    save.finish().unwrap();
    let before = storage.diagnostics();
    let save = storage.begin_save().unwrap();
    save.accept(o).unwrap();
    let result = save.finish().unwrap();
    let after = storage.diagnostics();
    assert_eq!((result.inserted, result.reused), (0, 1));
    assert!(after.read_packs + after.payload_reads > before.read_packs + before.payload_reads);
}
