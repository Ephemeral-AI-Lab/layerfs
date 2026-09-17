//! Pooled physical metadata: real save, reopen and exact canonical readback.
//!
//! Supplied canonical inode leaves are sufficient here: these cases store real
//! leaves through the real save path, reopen the Store and reconstruct the exact
//! canonical bytes and identity. No filesystem-tree construction is involved.

mod support;

use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, INODE_VALUE_BYTES,
    LEAF_ROW_BYTES, MAXIMUM_LEAF_ROWS,
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
        // The recorded total is the encoded row width: an 8-byte serial plus the
        // 73-byte value, exactly as the reference encoder writes it.
        subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
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

/// The catalogue's group rows, in ordinal order, as `(first_ordinal, count)`.
fn catalogue_groups(path: &std::path::Path) -> Vec<(u32, u32)> {
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

/// One group per leaf, and the group capacity is never reached by this path.
///
/// The reviewed version of this case was named for crossing the group boundary at
/// 165 values, which the public path cannot do: a pooled leaf carries at most
/// `MAXIMUM_LEAF_ROWS` (100) values and the lane groups only the values one leaf
/// resolved, so a group is one leaf's own values. What is observable is asserted
/// here: three full leaves produce three groups of exactly one hundred values at
/// ordinals 1, 101 and 201, each leaf's pooled row is readable, and no group in the
/// catalogue exceeds the leaf page capacity.
#[test]
fn a_group_holds_one_leaf_and_never_reaches_the_group_capacity() {
    // This case documents that one leaf cannot fill a group.
    const { assert!(MAXIMUM_LEAF_ROWS < layerfs_storage::policy::VALUES_PER_GROUP) };
    let dir = TempDir::new("pool-boundary");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    let mut previous = None;
    let mut leaves = Vec::new();
    for round in 0..3_u64 {
        let first = 1 + round * 1_000;
        let values = (first..first + MAXIMUM_LEAF_ROWS as u64)
            .map(|index| value(InodeKind::RegularFile, 1, index))
            .collect::<Vec<_>>();
        let leaf = match previous {
            Some(base) => with_predecessor(leaf(round * 1_000 + 1, &values), base),
            None => leaf(1, &values),
        };
        let id = leaf.id();
        let outcome = save_one(&store, leaf).expect("leaf save");
        assert_eq!(outcome.pool.leaves, 1);
        assert_eq!(outcome.pool.new_values, MAXIMUM_LEAF_ROWS as u64);
        assert_eq!(
            outcome.pool.groups, 1,
            "one leaf's values are one group in round {round}"
        );
        let (read, _) = read_objects(&store, &[id]).expect("leaf read");
        assert_eq!(ObjectId::for_bytes(&read[0]), id);
        assert_eq!(read[0].len(), 44 + MAXIMUM_LEAF_ROWS * 81);
        previous = Some(id);
        leaves.push(id);
    }
    let groups = catalogue_groups(&path);
    assert_eq!(groups, vec![(1, 100), (101, 100), (201, 100)]);
    assert!(
        groups
            .iter()
            .all(|(_, count)| *count as usize <= MAXIMUM_LEAF_ROWS),
        "a group above the leaf page capacity exists: {groups:?}"
    );
    let (read, _) = read_objects(&store, &leaves).expect("read all leaves");
    assert_eq!(read.len(), 3);
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
    disabled(|_scope| operation.accept(leaf)).expect("accept");
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

/// Two leaves accepted by **one** save: the second reuses values the first pooled
/// moments earlier, in a group pack this save created but has not published.
///
/// This pins the ceiling the pooled lane must supply while selecting: the owner's
/// own ceiling includes every pack the operation created, so a value the save just
/// pooled is resolvable to its ordinal inside the same save. A ceiling of the
/// published watermark - or of the pack the save started from - refuses that group
/// and the save fails, which is the regression this case exists to catch.
#[test]
fn one_save_reuses_a_value_group_it_created_itself() {
    let dir = TempDir::new("pool-same-save-reuse");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    let shared: Vec<[u8; INODE_VALUE_BYTES]> = (0..8)
        .map(|index| value(InodeKind::RegularFile, 1, index))
        .collect();
    let first = leaf(1, &shared);
    let first_id = first.id();
    let mut second_values = shared.clone();
    second_values.extend((20..22).map(|index| value(InodeKind::RegularFile, 1, index)));
    let second = with_predecessor(leaf(1_000, &second_values), first_id);
    let second_id = second.id();
    let second_canonical = canonical_of(&second);

    let outcome = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        operation.accept(first)?;
        operation.accept(second)?;
        operation.finish(scope.child("storage.finish"))
    })
    .expect("one save carrying both leaves");
    assert_eq!(outcome.pool.leaves, 2);
    assert_eq!(
        outcome.pool.reused_values, 8,
        "the second leaf resolves the group this same save wrote"
    );
    assert_eq!(
        outcome.pool.new_values, 10,
        "eight values from the first leaf and two new ones from the second"
    );

    let (read, _) = read_objects(&store, &[second_id]).expect("read back");
    assert_eq!(ObjectId::for_bytes(&read[0]), second_id);
    assert_eq!(read[0], second_canonical);
}

/// A compressible group really takes the `Zstandard` branch, and the bytes the
/// reader gets back are the ones the writer put in.
///
/// Every other pooled fixture is BLAKE3-derived and deduplicated, so its group
/// bodies are incompressible and the compressed branch never executed. This case
/// pools values that share 65 of their 73 bytes, then reads the *pack directory
/// entry* the writer produced: the group must be recorded as `Zstandard`, its
/// stored bytes must be shorter than the decoded body, and the leaf must still
/// read back as its exact canonical bytes.
#[test]
fn a_compressible_group_is_stored_as_a_zstandard_frame() {
    let dir = TempDir::new("pool-compressed");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    // Distinct values that differ in eight bytes and share the other sixty-five.
    let constant_content = ObjectId::for_bytes(&[0x11_u8; 32]);
    let constant_metadata = ObjectId::for_bytes(&[0x22_u8; 8]);
    let values: Vec<[u8; INODE_VALUE_BYTES]> = (1..=MAXIMUM_LEAF_ROWS as u64)
        .map(|refs| {
            encode_inode_value(InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: refs,
                content_root: constant_content,
                metadata_root: constant_metadata,
            })
        })
        .collect();
    let pooled = leaf(1, &values);
    let id = pooled.id();
    let outcome = save_one(&store, pooled).expect("leaf save");
    assert_eq!(outcome.pool.groups, 1);
    drop(store);

    // The stored group, straight out of the pack and its directory.
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let (pack_id, group_number): (i64, i64) = connection
        .query_row(
            "SELECT pack_id, group_number FROM metadata_value_groups ORDER BY first_ordinal LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("catalogue row");
    let pack: Vec<u8> = connection
        .query_row(
            "SELECT data FROM object_packs WHERE pack_id = ?1",
            [pack_id],
            |row| row.get(0),
        )
        .expect("pack row");
    drop(connection);
    let header = layerfs_storage::pack::parse_header(&pack).expect("pack header");
    let view = layerfs_storage::pack::group_view(&pack, header, group_number as usize)
        .expect("group view");
    assert_eq!(
        view.codec,
        layerfs_storage::pack::GroupCodec::Zstandard,
        "a compressible group must be stored compressed"
    );
    assert!(
        view.end - view.start < view.decoded_length,
        "the stored frame is smaller than the body: {} of {}",
        view.end - view.start,
        view.decoded_length
    );

    // The reader decompresses it and still returns the exact canonical leaf.
    let reopened = open_store(&path);
    let (read, _) = read_objects(&reopened, &[id]).expect("read back");
    assert_eq!(ObjectId::for_bytes(&read[0]), id);
    let (again, _) = read_objects(&reopened, &[id]).expect("cached read back");
    assert_eq!(again[0], read[0]);
}

/// A damaged pooled delta leaf is refused, never silently returned.
#[test]
fn a_damaged_pooled_delta_leaf_is_refused() {
    let dir = TempDir::new("pool-tamper");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    let base_values = (0..100)
        .map(|index| value(InodeKind::RegularFile, 1, index))
        .collect::<Vec<_>>();
    let base = leaf(1, &base_values);
    let base_id = base.id();
    save_one(&store, base).expect("base leaf");
    // One changed value: the second leaf is a real COPY/INSERT delta.
    let mut changed = base_values.clone();
    changed[40] = value(InodeKind::RegularFile, 1, 9_999);
    let dependent = with_predecessor(leaf(1, &changed), base_id);
    let dependent_id = dependent.id();
    let outcome = save_one(&store, dependent).expect("dependent leaf");
    assert_eq!(
        outcome.pool.delta_leaves, 1,
        "the second leaf is a stored delta"
    );
    let (read, _) = read_objects(&store, &[dependent_id]).expect("read before tampering");
    assert_eq!(ObjectId::for_bytes(&read[0]), dependent_id);
    drop(store);

    // Flip the last byte of the pack that holds the delta record: whatever it
    // lands on - an instruction, an inserted value or the framing - the reader
    // must refuse the record rather than return different bytes.
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let pack_id: i64 = connection
        .query_row(
            "SELECT pack_id FROM objects WHERE object_id = ?1",
            [dependent_id.to_bytes().to_vec()],
            |row| row.get(0),
        )
        .expect("object row");
    let pack: Vec<u8> = connection
        .query_row(
            "SELECT data FROM object_packs WHERE pack_id = ?1",
            [pack_id],
            |row| row.get(0),
        )
        .expect("pack row");
    let mut damaged = pack.clone();
    let last = damaged.len() - 1;
    damaged[last] ^= 0xff;
    assert_eq!(
        connection
            .execute(
                "UPDATE object_packs SET data = ?1 WHERE pack_id = ?2",
                rusqlite::params![damaged, pack_id],
            )
            .expect("pack update"),
        1
    );
    drop(connection);
    let reopened = open_store(&path);
    let error = read_objects(&reopened, &[dependent_id]).expect_err("a damaged record is refused");
    assert!(
        matches!(error, StorageError::Integrity(_)),
        "expected an integrity refusal, got {error}"
    );
}

/// The pooled chain budget binds on both sides of the boundary.
///
/// The writer refuses to create a dependent whose chain would exceed the
/// canonical budget - the leaf is counted as `work_exceeded` and stored FULL - and
/// a reader handed a longer chain, which only tampering can produce, refuses it
/// with the reader's own work check instead of reconstructing bytes outside the
/// budget.
#[test]
fn a_pooled_chain_past_the_canonical_budget_is_refused_on_both_sides() {
    let dir = TempDir::new("pool-chain-work");
    let path = dir.store_path("pool");
    // A depth that lets a tampered chain reach the *work* check rather than the
    // depth check: the budget itself is depth-independent.
    let store = disabled(|scope| {
        Store::create(
            &path,
            StoragePolicy::frozen_default().with_metadata_depth(50),
            scope.child("store"),
        )
    })
    .expect("store");
    assert_eq!(store.capacities().metadata_chain_canonical_limit, 65_536);

    // Each leaf repeats its predecessor's hundred values except one, so every
    // trial wins and the chain really extends by one record per save.
    let mut carried: Vec<[u8; INODE_VALUE_BYTES]> = (0..MAXIMUM_LEAF_ROWS as u64)
        .map(|index| value(InodeKind::RegularFile, 1, index))
        .collect();
    let mut accepted = Vec::new();
    let mut refused = None;
    for round in 0..12_u64 {
        if round > 0 {
            carried[(round as usize) % MAXIMUM_LEAF_ROWS] =
                value(InodeKind::RegularFile, 1, 10_000 + round);
        }
        // The same serials every round: only the one changed value moves, so the
        // physical body differs in a single row and the trial can win.
        let object = match accepted.last().copied() {
            Some(base) => with_predecessor(leaf(1, &carried), base),
            None => leaf(1, &carried),
        };
        let id = object.id();
        let outcome = save_one(&store, object).expect("leaf save");
        if outcome.pool.work_exceeded > 0 {
            assert_eq!(
                outcome.pool.full_leaves, 1,
                "a refused chain is stored FULL, never stored outside its budget"
            );
            assert_eq!(outcome.pool.delta_leaves, 0);
            refused = Some(id);
            break;
        }
        if round == 0 {
            assert_eq!(outcome.pool.full_leaves, 1, "the chain root is stored FULL");
            assert_eq!(outcome.pool.new_values, 100);
        } else {
            assert_eq!(outcome.pool.delta_leaves, 1, "round {round} is a delta");
            assert_eq!(outcome.pool.new_values, 1, "round {round} adds one value");
        }
        accepted.push(id);
    }
    let refused = refused.expect("the canonical budget must bind within eleven links");
    assert!(accepted.len() >= 2, "no chain was built");
    let deepest = *accepted.last().expect("deepest accepted leaf");

    // Every chain the writer accepted is readable, including the deepest one.
    let (read, _) = read_objects(&store, &[deepest]).expect("deepest accepted chain");
    assert_eq!(ObjectId::for_bytes(&read[0]), deepest);
    drop(store);

    // Splice the refused leaf onto the accepted chain: the reader's own work
    // check must refuse the longer chain.
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let affected = connection
        .execute(
            "UPDATE objects SET base_object_id = ?2 WHERE object_id = ?1",
            rusqlite::params![refused.to_bytes().to_vec(), deepest.to_bytes().to_vec()],
        )
        .expect("row change");
    assert_eq!(affected, 1);
    drop(connection);
    let reopened = open_store(&path);
    let error = read_objects(&reopened, &[refused]).expect_err("a past-budget chain is refused");
    assert!(
        matches!(&error, StorageError::Integrity(what) if *what == "pooled chain work"),
        "expected the reader's work check, got {error}"
    );
    // The accepted chain is untouched and still readable under the same ceiling.
    let (read, _) = read_objects(&reopened, &[deepest]).expect("accepted chain after tampering");
    assert_eq!(ObjectId::for_bytes(&read[0]), deepest);
}

/// A deep pooled-metadata depth is usable, not merely accepted.
///
/// The writer used to charge every record its worst-case bound, which made any
/// accepted depth above fifteen unable to admit a chain: the policy accepted a
/// value the store could never honour. Both budgets are now charged what a read
/// actually pays - the chain's canonical sum and the reader's own encoded bytes
/// plus the dependent's record - so a depth the policy accepts is a depth a chain
/// can reach. This case builds a chain of fifty small leaves, one per save, reads
/// the deepest one back and checks the depth the policy reports is the depth the
/// chain used.
#[test]
fn a_deep_metadata_depth_admits_and_reads_a_fifty_link_chain() {
    let dir = TempDir::new("pool-deep-depth");
    let path = dir.store_path("pool");
    let store = disabled(|scope| {
        Store::create(
            &path,
            StoragePolicy::frozen_default().with_metadata_depth(50),
            scope.child("store"),
        )
    })
    .expect("store");
    assert_eq!(store.policy().metadata_delta_max_depth(), 50);

    // One row per leaf keeps the canonical budget small enough that depth, not the
    // budget, is what the chain can use.
    let mut ids = Vec::new();
    let mut previous = None;
    for round in 0..50_u64 {
        let values = [value(InodeKind::RegularFile, 1, 100_000 + round)];
        let object = match previous {
            Some(base) => with_predecessor(leaf(1, &values), base),
            None => leaf(1, &values),
        };
        let id = object.id();
        let outcome = save_one(&store, object).expect("leaf save");
        if round == 0 {
            assert_eq!(outcome.pool.full_leaves, 1);
        } else {
            assert_eq!(
                outcome.pool.delta_leaves, 1,
                "round {round} must extend the chain: {:?}",
                outcome.pool
            );
            assert_eq!(outcome.pool.work_exceeded, 0, "round {round}");
        }
        ids.push(id);
        previous = Some(id);
    }
    assert_eq!(store.pool_index_entries(), 50);

    // The deepest leaf reconstructs its whole fifty-link chain, and every leaf in
    // it stays readable on its own.
    let deepest = *ids.last().expect("deepest leaf");
    let (read, _) = read_objects(&store, &[deepest]).expect("deepest chain read");
    assert_eq!(ObjectId::for_bytes(&read[0]), deepest);
    assert_eq!(read[0].len(), 44 + 81);
    let (all, _) = read_objects(&store, &ids).expect("every leaf read");
    assert_eq!(all.len(), 50);
    for (id, bytes) in ids.iter().zip(all) {
        assert_eq!(ObjectId::for_bytes(&bytes), *id);
    }

    // Depth fifty is the configured cap and it is usable: the fifty-first record
    // is admitted. One level deeper is not, and that is a depth decision rather
    // than a budget refusal, so the leaf is stored in full with no trial and no
    // work charge.
    let at_cap = with_predecessor(
        leaf(1, &[value(InodeKind::RegularFile, 1, 200_000)]),
        deepest,
    );
    let at_cap_id = at_cap.id();
    let outcome = save_one(&store, at_cap).expect("leaf at the depth cap");
    assert_eq!(outcome.pool.trials, 1, "the configured cap is usable");
    assert_eq!(outcome.pool.delta_leaves, 1);
    assert_eq!(outcome.pool.work_exceeded, 0);

    let past = with_predecessor(
        leaf(1, &[value(InodeKind::RegularFile, 1, 300_000)]),
        at_cap_id,
    );
    let outcome = save_one(&store, past).expect("leaf past the depth cap");
    assert_eq!(outcome.pool.trials, 0, "an over-depth base is not acquired");
    assert_eq!(outcome.pool.work_exceeded, 0, "depth is not a work budget");
    assert_eq!(outcome.pool.full_leaves, 1);
    assert_eq!(outcome.pool.delta_leaves, 0);

    let (read, _) = read_objects(&store, &[at_cap_id]).expect("chain at the cap");
    assert_eq!(ObjectId::for_bytes(&read[0]), at_cap_id);
}

/// The pooled value cache is released wholesale at its declared bound.
///
/// Two hundred leaves of a hundred distinct values each decode 1.39 MiB of values,
/// well past the 512 KiB the reader may retain: the cache must release itself
/// rather than grow with the number of leaves, and every leaf must still
/// reconstruct exactly.
#[test]
fn the_pooled_value_cache_releases_at_its_declared_bound() {
    use layerfs_storage::encoding::codec::DecompressionWorkspace;
    use layerfs_storage::encoding::pool::PoolReader;
    use layerfs_storage::policy::POOLED_VALUE_CACHE_BYTES;
    use layerfs_storage::sqlite::lookup;

    let dir = TempDir::new("pool-value-cache");
    let path = dir.store_path("pool");
    let store = create_store(&path);
    let leaves = 200_u64;
    let mut ids = Vec::new();
    let mut previous = None;
    for step in 0..leaves {
        let values: Vec<[u8; INODE_VALUE_BYTES]> = (0..MAXIMUM_LEAF_ROWS as u64)
            .map(|index| value(InodeKind::RegularFile, 1, step * 100 + index))
            .collect();
        let object = match previous {
            Some(base) => with_predecessor(leaf(step * 100 + 1, &values), base),
            None => leaf(1, &values),
        };
        let id = object.id();
        save_one(&store, object).expect("leaf save");
        ids.push(id);
        previous = Some(id);
    }
    let capacities = store.capacities();
    let ceiling = {
        let connection = rusqlite::Connection::open(&path).expect("external connection");
        connection
            .query_row(
                "SELECT retained_pack_ceiling FROM store_policy WHERE id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("watermark")
    };
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let mut workspace = DecompressionWorkspace::new().expect("decode workspace");
    let mut reader = PoolReader::new();
    let mut peak_retained = 0_usize;
    let mut decoded_total = 0_u64;
    for id in &ids {
        let location = lookup::location(&connection, *id, i64::MAX)
            .expect("locator")
            .expect("the leaf is stored");
        let canonical = reader
            .leaf_canonical(&connection, &capacities, ceiling, &mut workspace, location)
            .expect("pooled reconstruction");
        assert_eq!(ObjectId::for_bytes(&canonical), *id);
        peak_retained = peak_retained.max(reader.retained_bytes());
        decoded_total += (MAXIMUM_LEAF_ROWS * INODE_VALUE_BYTES) as u64;
    }
    assert!(
        decoded_total > 2 * POOLED_VALUE_CACHE_BYTES as u64,
        "the fixture must decode more than the cache may hold twice over: {decoded_total} bytes"
    );
    assert!(
        peak_retained <= POOLED_VALUE_CACHE_BYTES,
        "the cache grew past its bound: {peak_retained} bytes"
    );
    assert!(peak_retained > 0, "the cache was never populated");
    // More values were decoded than the cache may hold twice over while it never
    // retained more than the bound, so it must have released itself at least once:
    // a cache that never released would have had to retain `decoded_total`.
    println!(
        "MEASURED pooled value cache: decoded_total={decoded_total} B, \
         peak_retained={peak_retained} B, bound={POOLED_VALUE_CACHE_BYTES} B, leaves={leaves}"
    );
}
