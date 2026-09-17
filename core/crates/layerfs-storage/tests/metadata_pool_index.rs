//! The bounded Store-owned ordered set: reuse, synchronization and invalidation.
//!
//! The set is a derivation, not a second index: the catalogue is authoritative,
//! the set is bounded, and a failure invalidates it whole. These cases check the
//! externally visible consequences - which ordinal a value resolves to, after a
//! reopen and after a failure - and report the retained size the Store owns.

mod support;

use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, INODE_VALUE_BYTES,
};
use layerfs_content::{
    AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance,
};
use layerfs_storage::StorageError;
use support::{create_store, disabled, open_store, read_objects, save_one, TempDir};

fn value(seed: u64) -> [u8; INODE_VALUE_BYTES] {
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&seed.to_be_bytes());
    encode_inode_value(InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&bytes),
        metadata_root: ObjectId::for_bytes(&[seed as u8; 8]),
    })
}

fn leaf(values: &[[u8; INODE_VALUE_BYTES]]) -> FinalizedObject {
    leaf_from(1, values)
}

fn leaf_from(first_serial: u64, values: &[[u8; INODE_VALUE_BYTES]]) -> FinalizedObject {
    let rows = values
        .iter()
        .enumerate()
        .map(|(index, value)| InodeLeafRow {
            serial: first_serial + index as u64,
            value: *value,
        })
        .collect::<Vec<_>>();
    let canonical = InodeLeaf {
        subtree_bytes: rows.len() as u64 * INODE_VALUE_BYTES as u64,
        rows,
    }
    .encode()
    .expect("canonical leaf");
    FinalizedObject::new(ObjectRole::InodeLeaf, canonical).expect("finalized")
}

fn with_predecessor(object: FinalizedObject, base: ObjectId) -> FinalizedObject {
    let mut predecessors = AdvisoryPredecessors::new();
    predecessors
        .push(base, PredecessorProvenance::OriginalBase)
        .expect("bounded predecessor");
    object.with_predecessors(predecessors)
}

/// The ordinal each value resolved to, read straight from the catalogue rows.
fn ordinals(path: &std::path::Path) -> Vec<(u32, u32)> {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    let mut statement = connection
        .prepare("SELECT first_ordinal, count FROM metadata_value_groups ORDER BY first_ordinal")
        .expect("catalogue query");
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)? as u32, row.get::<_, i64>(1)? as u32))
        })
        .expect("catalogue rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect");
    rows
}

#[test]
fn a_shared_value_keeps_the_smallest_ordinal_across_leaves_and_reopen() {
    let dir = TempDir::new("index-ordinals");
    let path = dir.store_path("index");
    let store = create_store(&path);
    let shared = (0..4).map(value).collect::<Vec<_>>();
    let mut first_values = shared.clone();
    first_values.extend((10..14).map(value));
    let first = leaf(&first_values);
    let first_id = first.id();
    let first_outcome = save_one(&store, first).expect("first");
    assert_eq!(first_outcome.pool.new_values, 8);
    let first_rows = ordinals(&path);
    assert_eq!(first_rows.len(), 1);
    assert_eq!(first_rows[0], (1, 8));

    // The same shared values in a different position and with new neighbours
    // resolve to the ordinals the first leaf assigned, never to new ones.
    let mut second_values = vec![value(20)];
    second_values.extend(shared.clone());
    second_values.extend((30..33).map(value));
    let second = with_predecessor(leaf(&second_values), first_id);
    let second_id = second.id();
    let second_outcome = save_one(&store, second).expect("second");
    assert_eq!(second_outcome.pool.reused_values, 4);
    assert_eq!(second_outcome.pool.new_values, 4);

    // After reopen the Store re-synchronizes from the catalogue and still reuses.
    drop(store);
    let reopened = open_store(&path);
    let mut third_values = shared.clone();
    let third = with_predecessor(leaf(&third_values), second_id);
    let third_id = third.id();
    let third_outcome = save_one(&reopened, third).expect("third");
    assert_eq!(third_outcome.pool.new_values, 0);
    assert_eq!(third_outcome.pool.reused_values, 4);
    assert_eq!(third_outcome.pool.groups, 0);

    // The index is populated after the third save and reports a bounded size.
    assert!(reopened.pool_index_entries() >= 8);
    assert!(reopened.pool_index_bytes() >= opened_index_floor());
    let (read, _) = read_objects(&reopened, &[first_id, second_id, third_id]).expect("read");
    for (id, bytes) in [first_id, second_id, third_id].iter().zip(read) {
        assert_eq!(ObjectId::for_bytes(&bytes), *id);
    }
    third_values.clear();
}

/// One retained entry costs at least its key plus its ordinal.
fn opened_index_floor() -> usize {
    8 * (std::mem::size_of::<(i64, u32)>() + 8)
}

#[test]
fn a_cold_store_synchronizes_from_the_catalogue_without_reassigning() {
    let dir = TempDir::new("index-cold");
    let path = dir.store_path("index");
    let values = (0..8).map(value).collect::<Vec<_>>();
    let store = create_store(&path);
    let first = leaf(&values);
    let first_id = first.id();
    save_one(&store, first).expect("first");
    drop(store);

    // A brand-new Store object starts with an empty set and must re-derive the
    // ordinals from the catalogue, not hand out new ones.
    let reopened = open_store(&path);
    assert_eq!(
        reopened.pool_index_entries(),
        0,
        "a fresh Store starts empty"
    );
    // A different body with the same values: reuse must come from the catalogue.
    let second = with_predecessor(leaf_from(1_000, &values), first_id);
    let second_id = second.id();
    let outcome = save_one(&reopened, second).expect("second");
    assert_eq!(outcome.pool.new_values, 0);
    assert_eq!(outcome.pool.reused_values, 8);
    assert_eq!(ordinals(&path).len(), 1, "no second group was written");
    let (read, _) = read_objects(&reopened, &[second_id]).expect("read");
    assert_eq!(ObjectId::for_bytes(&read[0]), second_id);
}

/// After a save fails mid-way, the set holds nothing from it.
///
/// The reviewed version of this case failed a save and then re-used values the
/// *first* save had already pooled, which the catalogue resolves on its own: it
/// would have passed with the phantom entries still present. This version pools
/// the failed save's own private values again, which is the only request that can
/// reach the phantom ordinals the failed save had assigned.
#[test]
fn a_failed_save_leaves_no_usable_state_in_the_set() {
    let dir = TempDir::new("index-invalidate");
    let path = dir.store_path("index");
    let store = create_store(&path);
    let values = (0..8).map(value).collect::<Vec<_>>();
    let first = leaf(&values);
    let first_id = first.id();
    save_one(&store, first).expect("first");
    let retained_groups = ordinals(&path);
    assert_eq!(retained_groups, vec![(1, 8)]);

    // A save that fails after the leaf was prepared: the second accepted object
    // carries a whole-file role over a foreign payload, so the wave fails after
    // the leaf's values were resolved, given ordinals and grouped.
    let private = (100..108).map(value).collect::<Vec<_>>();
    let bad = FinalizedObject::new(
        ObjectRole::WholeFile,
        layerfs_content::object::codec::encode_bytes_object(b"not a whole-file payload")
            .expect("envelope"),
    )
    .expect("finalized");
    let error = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("begin"))?;
        operation.accept(leaf(&private), scope.child("accept"))?;
        operation.accept(bad, scope.child("accept"))?;
        operation.finish(scope.child("finish"))
    })
    .unwrap_err();
    assert!(
        matches!(error, StorageError::Content(_) | StorageError::Integrity(_)),
        "got {error}"
    );
    // The failed attempt left the catalogue exactly as it was, and the retained
    // set holds no entry at all.
    assert_eq!(ordinals(&path), retained_groups);
    assert_eq!(
        store.pool_index_entries(),
        0,
        "the failed save's ordinals must not survive in the set"
    );

    // The same private values in a later save: every one of them is new, none of
    // them resolves to an ordinal the failed save had assigned by hand, and the
    // leaf is readable afterwards.
    let second = with_predecessor(leaf_from(1_000, &private), first_id);
    let second_id = second.id();
    let outcome = save_one(&store, second).expect("second");
    assert_eq!(
        outcome.pool.new_values, 8,
        "no phantom ordinals were reused"
    );
    assert_eq!(outcome.pool.reused_values, 0);
    assert_eq!(ordinals(&path), vec![(1, 8), (9, 8)]);
    let (read, _) = read_objects(&store, &[first_id, second_id]).expect("read");
    assert_eq!(read.len(), 2);
    for (id, bytes) in [first_id, second_id].iter().zip(read) {
        assert_eq!(ObjectId::for_bytes(&bytes), *id);
    }
}

#[test]
fn a_failed_save_removes_its_private_value_groups() {
    let dir = TempDir::new("index-cleanup");
    let path = dir.store_path("index");
    let store = create_store(&path);
    let retained = (0..6).map(value).collect::<Vec<_>>();
    let retained_leaf = leaf(&retained);
    let retained_id = retained_leaf.id();
    save_one(&store, retained_leaf).expect("retained");
    let groups_before = ordinals(&path).len();

    // Fail a save after it wrote value groups: the private groups and their
    // catalogue rows must not survive, and the retained ones must stay readable.
    let private = (100..108).map(value).collect::<Vec<_>>();
    let bad = FinalizedObject::new(
        ObjectRole::WholeFile,
        layerfs_content::object::codec::encode_bytes_object(b"not a whole-file payload")
            .expect("envelope"),
    )
    .expect("finalized");
    let error = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("begin"))?;
        operation.accept(leaf(&private), scope.child("accept"))?;
        operation.accept(bad, scope.child("accept"))?;
        operation.finish(scope.child("finish"))
    })
    .unwrap_err();
    assert!(matches!(
        error,
        StorageError::Content(_) | StorageError::Integrity(_)
    ));
    assert_eq!(
        ordinals(&path).len(),
        groups_before,
        "the failed save left catalogue rows behind"
    );
    let (read, _) = read_objects(&store, &[retained_id]).expect("retained read");
    assert_eq!(ObjectId::for_bytes(&read[0]), retained_id);
}

#[test]
fn the_retained_set_is_bounded_by_entries_not_by_file_size() {
    let dir = TempDir::new("index-bound");
    let path = dir.store_path("index");
    let store = create_store(&path);
    // Ten leaves of 100 values each: the set retains one entry per distinct value
    // and never one per row occurrence.
    let mut previous = None;
    for step in 0..10_u64 {
        let values = (0..100)
            .map(|index| value(step * 1_000 + index))
            .collect::<Vec<_>>();
        let object = leaf(&values);
        let object = match previous {
            Some(base) => with_predecessor(object, base),
            None => object,
        };
        let id = object.id();
        save_one(&store, object).expect("leaf");
        previous = Some(id);
    }
    let entries = store.pool_index_entries();
    let bytes = store.pool_index_bytes();
    assert!(entries >= 1_000, "retained {entries} entries");
    assert!(
        bytes <= 131_072 * 16 + 1_024,
        "retained {bytes} bytes exceeds one entry's worth per retained value"
    );
}
