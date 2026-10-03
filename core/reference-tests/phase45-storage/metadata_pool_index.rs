//! The bounded Store-owned ordered set: reuse, synchronization and invalidation.
//!
//! The set is a derivation, not a second index: the catalogue is authoritative,
//! the set is bounded, and a failure invalidates it whole. These cases check the
//! externally visible consequences - which ordinal a value resolves to, after a
//! reopen and after a failure - and report the retained size the Store owns.

mod support;

use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, INODE_VALUE_BYTES,
    LEAF_ROW_BYTES,
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
        // The recorded total is the encoded row width: an 8-byte serial plus the
        // 73-byte value, exactly as the reference encoder writes it.
        subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
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
fn a_failed_save_cannot_change_the_published_candidate_set() {
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
        operation.accept(leaf(&private))?;
        operation.accept(bad)?;
        operation.finish(scope.child("finish"))
    })
    .unwrap_err();
    assert!(
        matches!(error, StorageError::Content(_) | StorageError::Integrity(_)),
        "got {error}"
    );
    // The failed attempt left the published catalogue and its candidate set intact.
    assert_eq!(ordinals(&path), retained_groups);
    assert_eq!(
        store.pool_index_entries(),
        8,
        "only the previously published ordinals remain"
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
    assert_eq!(
        ordinals(&path),
        vec![(1, 8), (17, 8)],
        "aborted ordinal reservations are never reused"
    );
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
        operation.accept(leaf(&private))?;
        operation.accept(bad)?;
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

#[test]
fn the_ordinal_ceiling_reports_itself_instead_of_wrapping_to_zero() {
    // The pooled ordinal column is a `u32` and the schema allows a group to end
    // exactly at `2^32`. The exclusive end of a full catalogue is therefore not
    // assignable, and the old accessor returned it through a `u32` cast, which is
    // zero: the next group was inserted at ordinal zero and the failure surfaced
    // later as an unrelated "metadata group row" or "metadata index chronology"
    // integrity error. The ceiling now names itself.
    use layerfs_storage::sqlite::connection::open as open_connection;
    use layerfs_storage::sqlite::pool::{insert_group, next_ordinal, ordinal_end, ValueGroupRow};

    let dir = TempDir::new("ordinal-ceiling");
    let path = dir.store_path("ordinal");
    let store = create_store(&path);
    drop(store);
    let connection = open_connection(&path, false).expect("connection");
    connection
        .execute("INSERT INTO saves(save_id,active_slot) VALUES(1,1)", [])
        .unwrap();
    connection
        .execute(
            "INSERT INTO object_packs (pack_id, save_id, data) VALUES (1, 1, zeroblob(32))",
            [],
        )
        .expect("pack row");
    let digest = ObjectId::for_bytes(b"ordinal-ceiling");

    // A catalogue that already reaches the last assignable ordinal: the cursor is
    // `u32::MAX`, which is still a valid ordinal to hand out.
    insert_group(
        &connection,
        &ValueGroupRow {
            first_ordinal: u32::MAX - 1,
            count: 1,
            pack_id: 1,
            group_number: 0,
            digest,
        },
    )
    .expect("a group ending at the last assignable ordinal");
    assert_eq!(
        ordinal_end(&connection).expect("end cursor"),
        u64::from(u32::MAX)
    );
    assert_eq!(
        next_ordinal(&connection).expect("the last ordinal is still free"),
        u32::MAX,
        "the last ordinal must still be handed out"
    );

    // A catalogue whose every ordinal is assigned: the cursor is one past the end
    // of the space, which is not representable as an ordinal.
    connection
        .execute("DELETE FROM metadata_value_groups", [])
        .expect("clear");
    insert_group(
        &connection,
        &ValueGroupRow {
            first_ordinal: u32::MAX,
            count: 1,
            pack_id: 1,
            group_number: 0,
            digest,
        },
    )
    .expect("a group ending exactly at the ceiling");
    assert_eq!(ordinal_end(&connection).expect("end cursor"), 1_u64 << 32);
}

#[test]
fn the_ordinal_ceiling_refuses_a_full_space_with_its_own_reason() {
    use layerfs_storage::sqlite::connection::open as open_connection;
    use layerfs_storage::sqlite::pool::{insert_group, next_ordinal, ValueGroupRow};

    let dir = TempDir::new("ordinal-full");
    let path = dir.store_path("ordinal");
    let store = create_store(&path);
    drop(store);
    let connection = open_connection(&path, false).expect("connection");
    connection
        .execute("INSERT INTO saves(save_id,active_slot) VALUES(1,1)", [])
        .unwrap();
    connection
        .execute(
            "INSERT INTO object_packs (pack_id, save_id, data) VALUES (1, 1, zeroblob(32))",
            [],
        )
        .expect("pack row");
    // A group that ends exactly at `2^32` exhausts the ordinal space.
    insert_group(
        &connection,
        &ValueGroupRow {
            first_ordinal: u32::MAX,
            count: 1,
            pack_id: 1,
            group_number: 0,
            digest: ObjectId::for_bytes(b"ordinal-full"),
        },
    )
    .expect("group ending at the ceiling");
    let error = next_ordinal(&connection).expect_err("the space is exhausted");
    assert!(
        matches!(error, StorageError::Integrity("metadata ordinal maximum")),
        "the ceiling produced {error}"
    );
}

#[test]
fn one_leaf_may_hold_every_row_the_pooled_value_memo_is_bounded_by() {
    // The per-save ordinal memo is bounded by one leaf's row count. A leaf at that
    // exact bound is admitted, so the bound is a real ceiling and not an off-by-one
    // refusal.
    let dir = TempDir::new("index-full-leaf");
    let path = dir.store_path("index");
    let store = create_store(&path);
    let values = (0..100).map(value).collect::<Vec<_>>();
    let outcome = save_one(&store, leaf(&values)).expect("a full leaf is admitted");
    assert_eq!(outcome.pool.leaves, 1);
    assert_eq!(outcome.pool.new_values, 100);
    assert_eq!(ordinals(&path), vec![(1, 100)]);
}

#[test]
fn the_last_ordinal_can_be_published_and_the_next_reservation_is_refused() {
    let dir = TempDir::new("ordinal-final-reservation");
    let path = dir.store_path("last");
    let store = create_store(&path);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE store_policy SET next_ordinal=?1,metadata_window_start=?1 WHERE id=1",
            [i64::from(u32::MAX)],
        )
        .unwrap();
    let first = leaf(&[value(1)]);
    let id = first.id();
    save_one(&store, first).unwrap();
    assert_eq!(ordinals(&path), vec![(u32::MAX, 1)]);
    assert_eq!(read_objects(&store, &[id]).unwrap().0.len(), 1);
    let error = save_one(&store, leaf_from(2, &[value(2)])).unwrap_err();
    assert!(matches!(
        error,
        StorageError::Integrity("metadata ordinal maximum")
    ));
    assert_eq!(read_objects(&store, &[id]).unwrap().0.len(), 1);
}
