//! Inode values are stored rows, one an ordinal. These tests count the work
//! of reading and saving pooled leaves over them and prove that a wrong or
//! absent row can only fail a read: the work follows the leaf, never the
//! number of Saves that came before it.
#[path = "support/memory_metadata.rs"]
mod metadata;
use layerfs_content::{
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
    },
    AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance,
};
use layerfs_storage::{policy::READ_OBJECT_LIMIT, Storage};
use std::sync::Arc;

const ROWS: u64 = 8;

/// A leaf of `ROWS` rows; its first `rewrites` rows carry rewritten values.
fn leaf(first_serial: u64, rewrites: u64) -> FinalizedObject {
    let rows: Vec<_> = (0..ROWS)
        .map(|row| InodeLeafRow {
            serial: first_serial + row,
            value: encode_inode_value(InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: ObjectId::for_bytes(
                    &[
                        &(first_serial + row).to_be_bytes()[..],
                        &[u8::from(row < rewrites)],
                    ]
                    .concat(),
                ),
                metadata_root: layerfs_content::object::inode_leaf::InodeMetadata::Root(
                    ObjectId::for_bytes(b"metadata"),
                ),
            }),
        })
        .collect();
    let canonical = InodeLeaf {
        subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
        rows,
    }
    .encode()
    .unwrap();
    FinalizedObject::new(ObjectRole::InodeLeaf, canonical).unwrap()
}
/// The same leaf, naming the stored leaf it replaces.
fn successor(object: FinalizedObject, replaced: &FinalizedObject) -> FinalizedObject {
    let mut named = AdvisoryPredecessors::new();
    named
        .push(replaced.id(), PredecessorProvenance::OriginalBase)
        .unwrap();
    object.with_predecessors(named)
}
fn value_lookups(metadata: &metadata::MemoryMetadata) -> usize {
    metadata
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(name, _)| *name == "inode_values")
        .count()
}
/// `saves` Saves of one leaf; each rewrites one more row and names the last.
fn history(metadata: &Arc<metadata::MemoryMetadata>, saves: u64) -> Vec<FinalizedObject> {
    let mut leaves = Vec::new();
    for rewrites in 0..saves {
        let object = match leaves.last() {
            Some(last) => successor(leaf(1, rewrites), last),
            None => leaf(1, 0),
        };
        let store = Storage::new(metadata.clone()).unwrap();
        let save = store.begin_save().unwrap();
        save.accept(object.clone()).unwrap();
        let saved = save.finish().unwrap();
        assert_eq!(saved.pool.leaves, 1);
        // Only the rewritten row is a new stored value.
        let new_values = if rewrites == 0 { ROWS } else { 1 };
        assert_eq!(saved.pool.new_values, new_values, "save {rewrites}");
        assert_eq!(saved.pool.reused_values, ROWS - new_values);
        leaves.push(object);
    }
    leaves
}

#[test]
fn a_changed_leaf_stores_one_row_for_each_changed_value() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let leaves = history(&metadata, 7);
    assert_eq!(metadata.state.lock().unwrap().values.len() as u64, ROWS + 6);
    // A reader and a Save each resolve the last leaf, whose values were
    // written by seven Saves and whose chain is six deltas deep, with one
    // value lookup: the whole chain's ordinals in one call.
    let handle = Storage::new(metadata.clone()).unwrap();
    let last = leaves.last().unwrap();
    let before = value_lookups(&metadata);
    assert_eq!(
        handle.reader().unwrap().read_objects(&[last.id()]).unwrap(),
        vec![last.canonical().to_vec()]
    );
    assert_eq!(value_lookups(&metadata) - before, 1);
    let save = handle.begin_save().unwrap();
    let before = value_lookups(&metadata);
    assert_eq!(
        save.read_objects(&[last.id()]).unwrap(),
        vec![last.canonical().to_vec()]
    );
    assert_eq!(value_lookups(&metadata) - before, 1);
}

#[test]
fn a_first_save_reads_nothing_of_the_history_it_does_not_change() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    history(&metadata, 7);
    // A handle that has saved nothing saves an unrelated new leaf: no earlier
    // pack is acquired and no stored value is looked up.
    let fresh = Storage::new(metadata.clone()).unwrap();
    metadata.acquired_pack_ids.lock().unwrap().clear();
    metadata.calls.lock().unwrap().clear();
    let save = fresh.begin_save().unwrap();
    save.accept(leaf(1_000, 0)).unwrap();
    let saved = save.finish().unwrap();
    assert_eq!(saved.pool.new_values, ROWS);
    assert_eq!(value_lookups(&metadata), 0);
    assert_eq!(
        *metadata.acquired_pack_ids.lock().unwrap(),
        Vec::<i64>::new()
    );
}

#[test]
fn one_save_stores_a_repeated_value_once() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let store = Storage::new(metadata.clone()).unwrap();
    let save = store.begin_save().unwrap();
    // The second leaf holds the same eight values under other serials, as
    // identical files of an unpacked dependency tree do.
    let first = leaf(1, 0);
    let rows: Vec<_> = InodeLeaf::decode(first.canonical())
        .unwrap()
        .rows
        .into_iter()
        .map(|row| InodeLeafRow {
            serial: row.serial + 100,
            value: row.value,
        })
        .collect();
    let second = FinalizedObject::new(
        ObjectRole::InodeLeaf,
        InodeLeaf {
            subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
            rows,
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    save.accept(first.clone()).unwrap();
    save.accept(second.clone()).unwrap();
    let saved = save.finish().unwrap();
    assert_eq!(saved.pool.new_values, ROWS);
    assert_eq!(saved.pool.reused_values, ROWS);
    assert_eq!(metadata.state.lock().unwrap().values.len() as u64, ROWS);
    let reading = Storage::new(metadata).unwrap();
    assert_eq!(
        reading
            .reader()
            .unwrap()
            .read_objects(&[first.id(), second.id()])
            .unwrap(),
        vec![first.canonical().to_vec(), second.canonical().to_vec()]
    );
}

#[test]
fn a_wrong_or_absent_stored_value_fails_the_read() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let leaves = history(&metadata, 3);
    let read = |object: &FinalizedObject| {
        Storage::new(metadata.clone())
            .unwrap()
            .reader()
            .unwrap()
            .read_objects(&[object.id()])
    };
    let last = leaves.last().unwrap();
    assert_eq!(read(last).unwrap(), vec![last.canonical().to_vec()]);
    // One flipped bit in a row only the first leaf of the chain names: the
    // last leaf does not use that value, and still refuses, because every
    // element of its chain is checked against its own identity.
    let untouched = metadata.state.lock().unwrap().values[&1];
    metadata.state.lock().unwrap().values.get_mut(&1).unwrap()[40] ^= 1;
    for object in &leaves {
        let error = read(object).unwrap_err();
        assert!(
            error.to_string().contains("pooled canonical identity"),
            "{error}"
        );
    }
    // A value of another row in its place is refused the same way.
    let other = metadata.state.lock().unwrap().values[&2];
    metadata.state.lock().unwrap().values.insert(1, other);
    assert!(read(last)
        .unwrap_err()
        .to_string()
        .contains("pooled canonical identity"));
    // An absent row is a failure, never an omission.
    metadata.state.lock().unwrap().values.remove(&1);
    assert!(read(last)
        .unwrap_err()
        .to_string()
        .contains("metadata ordinal missing"));
    metadata.state.lock().unwrap().values.insert(1, untouched);
    assert_eq!(read(last).unwrap(), vec![last.canonical().to_vec()]);
}

#[test]
fn one_handle_reads_any_number_of_leaves_at_one_lookup_each() {
    // More leaves, and more stored values, than any count a handle remembers.
    const LEAVES: u64 = READ_OBJECT_LIMIT as u64 / 2 + 64;
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let mut last = Vec::new();
    for rewrites in [0, 1] {
        let store = Storage::new(metadata.clone()).unwrap();
        let save = store.begin_save().unwrap();
        let objects: Vec<_> = (0..LEAVES)
            .map(|index| {
                let object = leaf(1 + index * ROWS, rewrites);
                match last.get(index as usize) {
                    Some(replaced) => successor(object, replaced),
                    None => object,
                }
            })
            .collect();
        for object in &objects {
            save.accept(object.clone()).unwrap();
        }
        let saved = save.finish().unwrap();
        assert_eq!(saved.pool.leaves, LEAVES);
        let new_values = if rewrites == 0 { ROWS } else { 1 };
        assert_eq!(saved.pool.new_values, LEAVES * new_values);
        last = objects;
    }
    assert!(metadata.state.lock().unwrap().values.len() > 2 * READ_OBJECT_LIMIT);
    // One handle for every demand, one reader a demand, as a daemon's fixed
    // read handle serves them.
    let handle = Storage::new(metadata.clone()).unwrap();
    metadata.calls.lock().unwrap().clear();
    for (index, object) in last.iter().enumerate() {
        let before = value_lookups(&metadata);
        let bytes = handle
            .reader()
            .and_then(|reader| reader.read_objects(&[object.id()]))
            .unwrap_or_else(|error| panic!("leaf {index}: {error:?}"));
        assert_eq!(bytes, vec![object.canonical().to_vec()]);
        assert_eq!(value_lookups(&metadata) - before, 1, "leaf {index}");
    }
    assert_eq!(value_lookups(&metadata) as u64, LEAVES);
}
