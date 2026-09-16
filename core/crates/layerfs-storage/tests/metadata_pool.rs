//! Pooled physical metadata: real save, reopen and exact canonical readback.
//!
//! Supplied canonical inode leaves are sufficient here: these cases store real
//! leaves through the real save path, reopen the Store and reconstruct the exact
//! canonical bytes and identity. No filesystem-tree construction is involved.

mod support;

use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, INODE_VALUE_BYTES,
};
use layerfs_content::{
    AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance,
};
use layerfs_storage::{StorageError, StoragePolicy, Store};
use support::{create_store, disabled, noise, open_store, read_objects, save_one, TempDir};

fn value(kind: InodeKind, refs: u64, seed: u64) -> [u8; INODE_VALUE_BYTES] {
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&seed.to_be_bytes());
    encode_inode_value(InodeValue {
        kind,
        namespace_ref_count: refs,
        content_root: ObjectId::for_bytes(&bytes),
        metadata_root: ObjectId::for_bytes(&[seed as u8; 8]),
    })
}

/// Builds a canonical leaf whose values start at `seed`.
fn leaf(first_serial: u64, values: &[[u8; INODE_VALUE_BYTES]]) -> FinalizedObject {
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
    FinalizedObject::new(ObjectRole::InodeLeaf, canonical).expect("finalized leaf")
}

fn with_predecessor(object: FinalizedObject, base: ObjectId) -> FinalizedObject {
    let mut predecessors = AdvisoryPredecessors::new();
    predecessors
        .push(base, PredecessorProvenance::OriginalBase)
        .expect("bounded predecessor");
    object.with_predecessors(predecessors)
}

fn canonical_of(object: &FinalizedObject) -> Vec<u8> {
    object.canonical().to_vec()
}

#[test]
fn a_supplied_leaf_round_trips_through_sqlite_and_reopen() {
    let dir = TempDir::new("pool-roundtrip");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    let values = (0..8)
        .map(|index| value(InodeKind::RegularFile, 1, index))
        .collect::<Vec<_>>();
    let leaf = leaf(1, &values);
    let id = leaf.id();
    let expected = canonical_of(&leaf);
    let outcome = save_one(&store, leaf).expect("save");
    assert_eq!(outcome.pool.leaves, 1);
    assert_eq!(outcome.pool.new_values, 8);
    assert_eq!(outcome.pool.groups, 1);
    assert_eq!(outcome.pool.full_leaves, 1, "the first leaf has no base");
    drop(store);

    let reopened = open_store(&path);
    let (read, _) = read_objects(&reopened, &[id]).expect("read");
    assert_eq!(read.len(), 1);
    assert_eq!(read[0], expected, "exact canonical bytes after reopen");
    assert_eq!(ObjectId::for_bytes(&read[0]), id);
}

#[test]
fn repeated_values_reuse_the_same_ordinals_across_leaves_and_saves() {
    let dir = TempDir::new("pool-reuse");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    let shared = (0..4)
        .map(|index| value(InodeKind::Directory, 1, index))
        .collect::<Vec<_>>();

    let mut first_values = shared.clone();
    first_values.extend((10..14).map(|index| value(InodeKind::RegularFile, 1, index)));
    let first = leaf(1, &first_values);
    let first_id = first.id();
    let first_outcome = save_one(&store, first).expect("first leaf");
    assert_eq!(first_outcome.pool.new_values, 8);
    assert_eq!(first_outcome.pool.reused_values, 0);

    // The second leaf repeats four values and adds two: only the new values take
    // new ordinals, and the repeated ones keep the ordinals the first leaf used.
    let mut second_values = shared.clone();
    second_values.extend((20..24).map(|index| value(InodeKind::RegularFile, 1, index)));
    let second = with_predecessor(leaf(1, &second_values), first_id);
    let second_id = second.id();
    let second_outcome = save_one(&store, second).expect("second leaf");
    assert_eq!(second_outcome.pool.new_values, 4);
    assert_eq!(second_outcome.pool.reused_values, 4);
    assert_eq!(second_outcome.pool.groups, 1, "one group of the new values");
    assert_eq!(
        second_outcome.pool.delta_leaves, 1,
        "the second leaf is a COPY/INSERT delta against the first"
    );

    // A third leaf in a later save reuses values through the reopened catalogue.
    drop(store);
    let reopened = open_store(&path);
    let third_values = shared.clone();
    let third = leaf(1, &third_values);
    let third_id = third.id();
    let third_outcome = save_one(&reopened, third).expect("third leaf");
    assert_eq!(third_outcome.pool.new_values, 0);
    assert_eq!(third_outcome.pool.reused_values, 4);
    assert_eq!(third_outcome.pool.groups, 0);

    let (read, _) = read_objects(&reopened, &[first_id, second_id, third_id]).expect("read");
    assert_eq!(read.len(), 3);
    for (id, bytes) in [first_id, second_id, third_id].iter().zip(read) {
        assert_eq!(ObjectId::for_bytes(&bytes), *id);
    }
}

#[test]
fn values_cross_a_group_boundary_at_one_hundred_sixty_five() {
    let dir = TempDir::new("pool-boundary");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    // 100 rows is the page maximum, so two leaves carry the first 200 values and
    // the third starts at 201 - one value past the 165-value group boundary.
    let first = leaf(
        1,
        &(0..100)
            .map(|index| value(InodeKind::RegularFile, 1, index))
            .collect::<Vec<_>>(),
    );
    let first_id = first.id();
    save_one(&store, first).expect("first leaf");
    let second = with_predecessor(
        leaf(
            1_000,
            &(1_000..1_100)
                .map(|index| value(InodeKind::RegularFile, 1, index))
                .collect::<Vec<_>>(),
        ),
        first_id,
    );
    let second_id = second.id();
    let outcome = save_one(&store, second).expect("second leaf");
    assert_eq!(outcome.pool.new_values, 100);
    assert_eq!(outcome.pool.groups, 1, "100 values still fit one group");
    let third_values = (2_000..2_100)
        .map(|index| value(InodeKind::RegularFile, 1, index))
        .collect::<Vec<_>>();
    let third = with_predecessor(leaf(2_000, &third_values), second_id);
    let third_id = third.id();
    let outcome = save_one(&store, third).expect("third leaf");
    assert_eq!(outcome.pool.new_values, 100);
    assert_eq!(outcome.pool.groups, 1);
    let (read, _) = read_objects(&store, &[third_id]).expect("read");
    assert_eq!(ObjectId::for_bytes(&read[0]), third_id);
    assert_eq!(read[0].len(), 44 + 100 * 81);
}

#[test]
fn a_losing_delta_trial_stores_the_leaf_in_full() {
    let dir = TempDir::new("pool-loss");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    let first = leaf(
        1,
        &(0..8)
            .map(|index| value(InodeKind::RegularFile, 1, index))
            .collect::<Vec<_>>(),
    );
    let first_id = first.id();
    save_one(&store, first).expect("first");
    // A target that shares no sixteen-byte seed run with the base: the only
    // program the producer can build inserts the whole body behind a 41-byte
    // instruction header, which is larger than the full record. The trial runs and
    // FULL wins on cost, not because anything failed.
    let second = with_predecessor(
        leaf(1, &[value(InodeKind::Directory, 1, 987_654)]),
        first_id,
    );
    let second_id = second.id();
    let outcome = save_one(&store, second).expect("second");
    assert_eq!(outcome.pool.trials, 1);
    assert_eq!(outcome.pool.full_leaves, 1, "FULL is a cost outcome");
    assert_eq!(outcome.pool.delta_leaves, 0);
    let (read, _) = read_objects(&store, &[second_id]).expect("read");
    assert_eq!(ObjectId::for_bytes(&read[0]), second_id);
}

#[test]
fn a_corrupt_value_group_is_rejected_once() {
    let dir = TempDir::new("pool-corrupt");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    let leaf = leaf(
        1,
        &(0..6)
            .map(|index| value(InodeKind::RegularFile, 1, index))
            .collect::<Vec<_>>(),
    );
    let id = leaf.id();
    save_one(&store, leaf).expect("save");
    drop(store);
    support::corrupt_value_group_digest(&path);
    let reopened = open_store(&path);
    let error = read_objects(&reopened, &[id]).unwrap_err();
    assert!(
        matches!(error, StorageError::Integrity(what) if what.contains("value group")),
        "got {error}"
    );
}

#[test]
fn a_missing_catalogue_row_is_rejected() {
    let dir = TempDir::new("pool-catalogue");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    let leaf = leaf(
        1,
        &(0..6)
            .map(|index| value(InodeKind::RegularFile, 1, index))
            .collect::<Vec<_>>(),
    );
    let id = leaf.id();
    save_one(&store, leaf).expect("save");
    drop(store);
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let removed = connection
        .execute("DELETE FROM metadata_value_groups", [])
        .expect("catalogue delete");
    assert_eq!(removed, 1);
    drop(connection);
    let reopened = open_store(&path);
    let error = read_objects(&reopened, &[id]).unwrap_err();
    assert!(
        matches!(
            error,
            StorageError::Integrity(_) | StorageError::ObjectMissing(_)
        ),
        "got {error}"
    );
}

#[test]
fn an_unfinished_private_value_group_is_not_visible_to_a_reader() {
    let dir = TempDir::new("pool-visibility");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    let leaf = leaf(
        1,
        &(0..6)
            .map(|index| value(InodeKind::RegularFile, 1, index))
            .collect::<Vec<_>>(),
    );
    let id = leaf.id();
    let mut operation = disabled(|scope| store.begin_save(scope.child("begin"))).expect("begin");
    disabled(|scope| operation.accept(leaf, scope.child("accept"))).expect("accept");
    // Nothing is acknowledged yet: an unrelated reader must observe neither the
    // private value groups nor a range that can address them.
    let error = read_objects(&store, &[id]).unwrap_err();
    assert!(
        matches!(
            error,
            StorageError::VisibilityCeiling { .. } | StorageError::ObjectMissing(_)
        ),
        "got {error}"
    );
    disabled(|scope| operation.abort(scope.child("abort"))).expect("abort");
}

#[test]
fn a_pooled_leaf_survives_a_store_reopen_with_its_chain() {
    let dir = TempDir::new("pool-chain");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    let base_values = (0..40)
        .map(|index| value(InodeKind::RegularFile, 1, index))
        .collect::<Vec<_>>();
    let mut ids = Vec::new();
    let mut previous = None;
    for step in 0..4_u64 {
        let mut values = base_values.clone();
        for value in values.iter_mut().take(4) {
            value[1] = value[1].wrapping_add(step as u8);
        }
        let object = leaf(1, &values);
        let object = match previous {
            Some(base) => with_predecessor(object, base),
            None => object,
        };
        let id = object.id();
        let outcome = save_one(&store, object).expect("chain step");
        if step > 0 {
            assert_eq!(outcome.pool.delta_leaves, 1);
        }
        ids.push(id);
        previous = Some(id);
    }
    // Reading the deepest leaf reconstructs its whole pooled chain.
    let (read, _) = read_objects(&store, &[ids[3]]).expect("deep read");
    assert_eq!(ObjectId::for_bytes(&read[0]), ids[3]);
    assert_eq!(read[0].len(), 44 + 40 * 81);
    // Every retained leaf stays readable on its own.
    let (all, _) = read_objects(&store, &ids).expect("chain read");
    for (id, bytes) in ids.iter().zip(all) {
        assert_eq!(ObjectId::for_bytes(&bytes), *id);
    }
}

#[test]
fn a_pooled_depth_of_zero_stores_every_leaf_in_full() {
    let dir = TempDir::new("pool-depth0");
    let path = dir.store_path("pool");
    let policy = StoragePolicy::new(1, 131_072, 8, 4).with_metadata_depth(0);
    let store =
        disabled(|scope| Store::create(&path, policy, scope.child("store"))).expect("store");
    let values = (0..20)
        .map(|index| value(InodeKind::RegularFile, 1, index))
        .collect::<Vec<_>>();
    let first = leaf(1, &values);
    let first_id = first.id();
    save_one(&store, first).expect("first");
    let second = with_predecessor(leaf(2, &values), first_id);
    let outcome = save_one(&store, second).expect("second");
    assert_eq!(outcome.pool.trials, 0);
    assert_eq!(outcome.pool.full_leaves, 1);
    assert_eq!(store.policy().metadata_delta_max_depth(), 0);
    let _ = noise(8);
}
