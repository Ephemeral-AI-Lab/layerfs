//! A wave whose pages name more payloads than the locator cache holds locates
//! them a cache's worth at a time, so admission and publication find each
//! reference still located instead of locating it again alone. An edit that
//! appends to a large file emits its mapping pages at the end of the Save,
//! naming every chunk the Save published earlier.
#[path = "support/memory_metadata.rs"]
mod metadata;

use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::{policy, Storage};
use std::sync::Arc;

fn chunk(n: u64) -> FinalizedObject {
    let canonical = layerfs_content::file::mapping::encode_chunk_object(&n.to_be_bytes()).unwrap();
    FinalizedObject::new(ObjectRole::Chunk, canonical).unwrap()
}

fn page(n: u64, named: &[FinalizedObject]) -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::FileState,
        layerfs_content::object::codec::encode_bytes_object(&n.to_be_bytes()).unwrap(),
    )
    .unwrap()
    .with_references(named.iter().map(FinalizedObject::id).collect())
}

#[test]
fn pages_naming_more_payloads_than_the_cache_holds_locate_them_in_pages() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let storage = Storage::new(metadata.clone()).unwrap();
    let payloads = policy::READ_OBJECT_LIMIT * 3 / 2;
    let chunks: Vec<_> = (0..payloads as u64).map(chunk).collect();
    // One page per 120 payloads, in a final wave after all of them.
    let pages: Vec<_> = chunks
        .chunks(120)
        .enumerate()
        .map(|(n, named)| page(n as u64, named))
        .collect();
    let save = storage.begin_save().unwrap();
    for object in &chunks {
        save.accept(object.clone()).unwrap();
    }
    let before = locates(&metadata);
    for page in &pages {
        save.accept(page.clone()).unwrap();
    }
    let outcome = save.finish().unwrap();
    let after = locates(&metadata);
    assert_eq!(outcome.inserted, (chunks.len() + pages.len()) as u64);
    // Admission and publication each ask for the pages' references in pages
    // of the cache bound: a handful of calls, not one per payload.
    let calls = &after[before.len()..];
    let named: usize = pages.iter().map(|page| page.references().len()).sum();
    assert!(calls.len() <= 8, "{} locate calls: {calls:?}", calls.len());
    assert!(
        calls.iter().sum::<usize>() <= 2 * (named + pages.len()),
        "{calls:?}"
    );
    let reader = Storage::new(metadata).unwrap();
    let reader = reader.reader().unwrap();
    for object in pages.iter().chain(&chunks[..8]) {
        assert_eq!(
            reader.read_objects(&[object.id()]).unwrap(),
            vec![object.canonical().to_vec()]
        );
    }
}

/// The ids each `locate` port call asked for, in call order.
fn locates(metadata: &metadata::MemoryMetadata) -> Vec<usize> {
    metadata
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(name, _)| *name == "locate")
        .map(|(_, ids)| *ids)
        .collect()
}
