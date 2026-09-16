//! Pooled-metadata dependency chains: which limit cuts the chain.
//!
//! A pooled leaf may be stored as a COPY/INSERT delta against an earlier leaf, so
//! reconstructing one value can walk a chain of records. Two limits bound that
//! walk: the depth policy (`metadata_delta_max_depth`, eight by default) and the
//! chain byte budgets. A producer must never create the dependency a later read
//! would refuse. These cases build real chains in both regimes and show which limit
//! binds in each: small records hit the depth cap, maximal records hit the canonical
//! byte budget first, and in both regimes the deepest admitted leaf reads back and
//! the object past the edge is stored in full and self-contained.

mod support;

use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, INODE_VALUE_BYTES,
};
use layerfs_content::{
    AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance,
};
use layerfs_storage::policy::METADATA_CHAIN_CANONICAL_LIMIT;
use layerfs_storage::Store;
use support::{create_store, read_objects, save_one, TempDir};

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

/// A leaf that shares every row but the last with its predecessor, so the pooled
/// representation is a small delta whenever a delta is admitted at all.
fn chained_leaf(rows_count: u64, last_seed: u64) -> (FinalizedObject, u64) {
    let mut values = (0..rows_count - 1).map(value).collect::<Vec<_>>();
    values.push(value(last_seed));
    let rows = values
        .iter()
        .enumerate()
        .map(|(index, value)| InodeLeafRow {
            serial: 1 + index as u64,
            value: *value,
        })
        .collect::<Vec<_>>();
    let canonical = InodeLeaf {
        subtree_bytes: rows.len() as u64 * INODE_VALUE_BYTES as u64,
        rows,
    }
    .encode()
    .expect("canonical leaf");
    let length = canonical.len() as u64;
    (
        FinalizedObject::new(ObjectRole::InodeLeaf, canonical).expect("finalized leaf"),
        length,
    )
}

fn with_predecessor(object: FinalizedObject, base: ObjectId) -> FinalizedObject {
    let mut predecessors = AdvisoryPredecessors::new();
    predecessors
        .push(base, PredecessorProvenance::OriginalBase)
        .expect("bounded predecessor");
    object.with_predecessors(predecessors)
}

/// Saves `count` chained leaves of `rows` rows each and returns their identities,
/// the per-save chain counters and the canonical length of one leaf.
fn chain(store: &Store, rows: u64, count: u64) -> (Vec<ObjectId>, Vec<(u64, u64, u64)>, u64) {
    let mut ids = Vec::new();
    let mut counters = Vec::new();
    let mut length = 0;
    let mut previous = None;
    for step in 0..count {
        let (object, canonical_length) = chained_leaf(rows, 10_000 + step);
        length = canonical_length;
        let object = match previous {
            Some(base) => with_predecessor(object, base),
            None => object,
        };
        let id = object.id();
        let outcome = save_one(store, object).expect("chained leaf");
        counters.push((
            outcome.pool.delta_leaves,
            outcome.pool.full_leaves,
            outcome.pool.work_exceeded,
        ));
        ids.push(id);
        previous = Some(id);
    }
    (ids, counters, length)
}

/// Removes one object's locator row, simulating an externally damaged Store.
fn delete_object(path: &std::path::Path, id: ObjectId) {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    let removed = connection
        .execute(
            "DELETE FROM objects WHERE object_id = ?1",
            [id.as_bytes().as_slice()],
        )
        .expect("delete object");
    assert_eq!(removed, 1, "the object must have been present");
}

/// True when the object's locator row exists.
fn object_exists(path: &std::path::Path, id: ObjectId) -> bool {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    let mut statement = connection
        .prepare("SELECT count(*) FROM objects WHERE object_id = ?1")
        .expect("prepare");
    let count: i64 = statement
        .query_row([id.as_bytes().as_slice()], |row| row.get(0))
        .expect("count");
    count == 1
}

#[test]
fn small_records_reach_the_depth_cap_and_the_next_edge_is_stored_in_full() {
    let dir = TempDir::new("chain-depth");
    let path = dir.store_path("chain");
    let store = create_store(&path);
    // Eight rows per leaf: a nine-record chain costs 9 x 679 = 6,111 canonical
    // bytes, far below the 65,536-byte budget, so only the depth policy can cut it.
    let (ids, counters, length) = chain(&store, 8, 10);
    assert!(9 * length < METADATA_CHAIN_CANONICAL_LIMIT);
    assert_eq!(
        counters[0],
        (0, 1, 0),
        "the first leaf has no base and is stored in full"
    );
    for (index, entry) in counters.iter().enumerate().take(9).skip(1) {
        assert_eq!(*entry, (1, 0, 0), "leaf {} must be a delta", index + 1);
    }
    assert_eq!(
        counters[9],
        (0, 1, 0),
        "a base that already carries eight edges is past the cap: stored in full"
    );

    // Every admitted leaf reconstructs exactly, including the deepest chain. The
    // Store's read counters report the requested record only - the pooled dependency
    // walk is bounded by the reader's own chain budgets, not reported here - so the
    // identity check is what shows the reconstruction was complete.
    let (read, counters_read) = read_objects(&store, &[ids[8]]).expect("depth-eight read");
    assert_eq!(ObjectId::for_bytes(&read[0]), ids[8]);
    assert_eq!(
        (counters_read.objects, counters_read.canonical_bytes),
        (1, length),
        "pooled dependency work is not part of these counters"
    );

    // The chain is real and is walked: removing an intermediate record makes every
    // leaf above it unreadable, while the leaves below it still read exactly.
    let hole = ids[4];
    delete_object(&path, hole);
    assert!(
        object_exists(&path, ids[8]),
        "the damaged leaf is still present"
    );
    let (read, _) = read_objects(&store, &[ids[3]]).expect("below the hole");
    assert_eq!(ObjectId::for_bytes(&read[0]), ids[3]);
    let error = read_objects(&store, &[ids[8]]).expect_err("above the hole");
    assert!(
        matches!(
            error,
            layerfs_storage::StorageError::ObjectMissing(_)
                | layerfs_storage::StorageError::Integrity(_)
        ),
        "got {error}"
    );

    // The leaf past the depth edge is self-contained: it reads exactly, with no
    // dependency on the damaged record at all.
    let (read, _) = read_objects(&store, &[ids[9]]).expect("self-contained read");
    assert_eq!(ObjectId::for_bytes(&read[0]), ids[9]);
}

#[test]
fn maximal_records_hit_the_canonical_budget_before_the_depth_cap() {
    let dir = TempDir::new("chain-budget");
    let path = dir.store_path("chain");
    let store = create_store(&path);
    // Hundred-row leaves are the largest admitted records: eight records already
    // cost 8 x 8,144 = 65,152 canonical bytes, so a ninth record would exceed the
    // 65,536-byte budget while the depth policy would still admit it.
    let (ids, counters, length) = chain(&store, 100, 10);
    assert!(
        8 * length <= METADATA_CHAIN_CANONICAL_LIMIT,
        "{length} bytes per record"
    );
    assert!(
        9 * length > METADATA_CHAIN_CANONICAL_LIMIT,
        "a ninth record must not fit the canonical budget"
    );
    assert_eq!(counters[0], (0, 1, 0));
    for (index, entry) in counters.iter().enumerate().take(8).skip(1) {
        assert_eq!(*entry, (1, 0, 0), "leaf {} must be a delta", index + 1);
    }
    assert_eq!(
        counters[8],
        (0, 1, 1),
        "the ninth record is refused by the chain budget, not by the depth cap"
    );

    // The deepest admitted chain reconstructs exactly and stays inside its budget.
    let (read, _) = read_objects(&store, &[ids[7]]).expect("budget-limited read");
    assert_eq!(ObjectId::for_bytes(&read[0]), ids[7]);
    // The refused record is self-contained, and the chain restarts behind it.
    let (read, _) = read_objects(&store, &[ids[8]]).expect("full read");
    assert_eq!(ObjectId::for_bytes(&read[0]), ids[8]);
    assert_eq!(counters[9], (1, 0, 0), "the chain continues one edge deep");
    let (read, _) = read_objects(&store, &[ids[9]]).expect("restart read");
    assert_eq!(ObjectId::for_bytes(&read[0]), ids[9]);

    // Removing the self-contained record breaks only the leaf that depends on it.
    delete_object(&path, ids[8]);
    let (read, _) = read_objects(&store, &[ids[7]]).expect("independent of the hole");
    assert_eq!(ObjectId::for_bytes(&read[0]), ids[7]);
    let error = read_objects(&store, &[ids[9]]).expect_err("dependent on the hole");
    assert!(
        matches!(
            error,
            layerfs_storage::StorageError::ObjectMissing(_)
                | layerfs_storage::StorageError::Integrity(_)
        ),
        "got {error}"
    );
}
