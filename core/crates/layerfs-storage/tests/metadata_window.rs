//! The retained pooled-metadata window: exact boundary, wholesale reset, eviction.
//!
//! The retained set is a bounded window over pooled values, not a cache of the
//! store: when a group would push it past its bound the whole window is released,
//! so an evicted value is written a second time rather than lost. These cases drive
//! the real boundary (`METADATA_INDEX_VALUES`, 131,072 entries), the group
//! arithmetic (`VALUES_PER_GROUP`, 165 values) and the catalogue replay after a
//! reopen, and then read the early objects back to prove nothing was lost.

mod support;

use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, INODE_VALUE_BYTES,
};
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::encoding::pool::PoolIndex;
use layerfs_storage::policy::{METADATA_INDEX_VALUES, VALUES_PER_GROUP};
use layerfs_storage::StorageError;
use support::{create_store, open_store, read_objects, save_one, TempDir};

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

/// Builds a canonical leaf whose rows start at `first_serial`.
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

/// `(first_ordinal, count)` of every value group in catalogue order.
fn groups(path: &std::path::Path) -> Vec<(u32, u32)> {
    let connection = rusqlite::Connection::open(path).expect("open catalogue");
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

/// Total pooled ordinals recorded in the catalogue.
fn ordinals(path: &std::path::Path) -> u64 {
    groups(path)
        .iter()
        .map(|(_, count)| u64::from(*count))
        .sum()
}

#[test]
fn the_window_retains_exactly_the_cap_and_resets_whole_groups() {
    // The bound is exact: filling it to the last entry retains everything, and the
    // next value releases the whole window instead of trimming an oldest entry.
    let mut index = PoolIndex::new();
    let group = (0..VALUES_PER_GROUP)
        .map(|position| value(position as u64))
        .collect::<Vec<_>>();
    let full_groups = METADATA_INDEX_VALUES / VALUES_PER_GROUP;
    let remainder = METADATA_INDEX_VALUES % VALUES_PER_GROUP;
    let mut next = 1_u32;
    for _ in 0..full_groups {
        index.note_group(next, &group).expect("full group");
        next += VALUES_PER_GROUP as u32;
    }
    assert_eq!(index.len(), full_groups * VALUES_PER_GROUP);
    assert!(remainder > 0, "the fixture must land on an exact remainder");

    let tail = (0..remainder)
        .map(|position| value(1_000_000 + position as u64))
        .collect::<Vec<_>>();
    index.note_group(next, &tail).expect("exact fill");
    assert_eq!(
        index.len(),
        METADATA_INDEX_VALUES,
        "the cap itself is retained, not evicted"
    );
    next += remainder as u32;

    // One value past the cap: the window resets wholesale before the group is
    // noted, so the retained set is exactly the group that crossed the bound.
    let crossing = vec![value(2_000_000)];
    index.note_group(next, &crossing).expect("crossing group");
    assert_eq!(index.len(), 1, "the window resets wholesale");
    assert_eq!(
        index.live_bytes(),
        std::mem::size_of::<(i64, u32)>() + 8,
        "one retained entry costs its key and ordinal"
    );

    // Chronology and empty groups are refused without disturbing the window.
    let error = index.note_group(1, &group).unwrap_err();
    assert!(matches!(error, StorageError::Integrity(_)), "got {error}");
    let error = index.note_group(next + 1, &[]).unwrap_err();
    assert!(matches!(error, StorageError::Integrity(_)), "got {error}");
    assert_eq!(index.len(), 1, "a refused note leaves the window alone");
    // The window keeps accumulating after a release: the crossing group's entry is
    // still retained, and the next group adds to it rather than starting over.
    index
        .note_group(next + 1, &group)
        .expect("the window accepts groups after a release");
    assert_eq!(index.len(), 1 + VALUES_PER_GROUP);
}

#[test]
fn crossing_the_window_evicts_early_ordinals_and_the_reopen_replays_the_window() {
    // Each admitted leaf contributes one value group of its own 100 values, so this
    // fixture writes 1,312 groups and 131,200 ordinals. At the 1,311th leaf the
    // retained set would be 131,000 + 100 = 131,100, past the 131,072 bound, so that
    // group releases the whole window: the retained set ends as the last two leaves,
    // 200 entries, and never as the first 131,072 values.
    let dir = TempDir::new("metadata-window");
    let path = dir.store_path("window");
    let store = create_store(&path);
    let leaves = 1_312_u64;
    let mut first_id = None;
    let mut last_values = Vec::new();
    for step in 0..leaves {
        let values = (0..100_u64)
            .map(|index| value(step * 100 + index))
            .collect::<Vec<_>>();
        if step == 0 {
            first_id = Some(leaf(1, &values).id());
        }
        if step == leaves - 1 {
            last_values = values.clone();
        }
        let object = leaf(step * 100 + 1, &values);
        save_one(&store, object).expect("leaf");
    }
    let first_id = first_id.expect("first leaf identity");
    let rows = groups(&path);
    assert_eq!(rows.len(), 1_312, "one group per admitted leaf");
    assert_eq!(rows[1_310], (131_001, 100), "the crossing group");
    assert_eq!(rows[1_311], (131_101, 100));
    assert_eq!(ordinals(&path), 131_200);
    assert_eq!(
        store.pool_index_entries(),
        200,
        "the window retains the groups after the crossing, not the oldest values"
    );
    assert!(store.pool_index_entries() < METADATA_INDEX_VALUES);

    // An evicted value is written again, never lost: the first leaf's values are no
    // longer candidates, so the save assigns new ordinals for them.
    let early = (0..100_u64).map(value).collect::<Vec<_>>();
    let surviving = leaf(200_000, &early);
    let surviving_id = surviving.id();
    let outcome = save_one(&store, surviving).expect("evicted value re-save");
    assert_eq!(outcome.pool.new_values, 100, "evicted values are rewritten");
    assert_eq!(outcome.pool.reused_values, 0);
    assert_eq!(outcome.pool.groups, 1);
    assert_eq!(ordinals(&path), 131_300);

    // Values still inside the window are reused.
    let reused = leaf(300_000, &last_values);
    let reused_id = reused.id();
    let outcome = save_one(&store, reused).expect("retained value re-save");
    assert_eq!(outcome.pool.reused_values, 100);
    assert_eq!(outcome.pool.new_values, 0);
    assert_eq!(outcome.pool.groups, 0, "no group was written");

    // The value that was written twice now resolves to its newest - and smallest
    // retained - ordinal, so no third copy appears.
    let again = leaf(400_000, &early);
    let again_id = again.id();
    let outcome = save_one(&store, again).expect("smallest retained ordinal");
    assert_eq!(outcome.pool.reused_values, 100);
    assert_eq!(outcome.pool.new_values, 0);
    assert_eq!(ordinals(&path), 131_300, "no second copy of the pair");

    // Evicted groups are still readable by identity: the window is a candidate set,
    // not the object graph.
    let (read, counters) =
        read_objects(&store, &[first_id, reused_id, again_id, surviving_id]).expect("readback");
    for (id, bytes) in [first_id, reused_id, again_id, surviving_id]
        .iter()
        .zip(&read)
    {
        assert_eq!(ObjectId::for_bytes(bytes), *id);
    }
    assert!(counters.objects >= 4);

    // Reopen: a fresh Store starts empty and re-derives the retained window from the
    // catalogue alone. The first leaf's values are found through the copy that was
    // written after the crossing (their newest and smallest retained ordinal), while
    // a value from an evicted group that was never rewritten is still a miss, which
    // proves the replay kept the producer's window instead of rewinding to ordinal 1.
    drop(store);
    let reopened = open_store(&path);
    assert_eq!(
        reopened.pool_index_entries(),
        0,
        "a fresh Store starts empty"
    );
    let late = leaf(500_000, &early);
    let late_id = late.id();
    let outcome = save_one(&reopened, late).expect("replayed window: rewritten value");
    assert_eq!(
        outcome.pool.reused_values, 100,
        "the replay must find the rewritten copy inside the window"
    );
    assert_eq!(outcome.pool.new_values, 0);
    assert_eq!(outcome.pool.groups, 0);
    assert_eq!(reopened.pool_index_entries(), 300);

    let evicted = (100..200_u64).map(value).collect::<Vec<_>>();
    let older = leaf(600_000, &evicted);
    let older_id = older.id();
    let outcome = save_one(&reopened, older).expect("replayed window: evicted value");
    assert_eq!(
        outcome.pool.new_values, 100,
        "the replay must not retain values from before the crossing"
    );
    assert_eq!(outcome.pool.reused_values, 0);
    assert_eq!(ordinals(&path), 131_400);
    assert_eq!(reopened.pool_index_entries(), 400);

    let probe = leaf(700_000, &last_values);
    let probe_id = probe.id();
    let outcome = save_one(&reopened, probe).expect("replayed window: retained value");
    assert_eq!(
        outcome.pool.reused_values, 100,
        "the replay found the window"
    );
    assert_eq!(outcome.pool.new_values, 0);
    assert_eq!(outcome.pool.groups, 0);
    assert_eq!(
        reopened.pool_index_entries(),
        400,
        "the cold-start replay reproduces the producer's window plus the new group"
    );

    let (read, _) = read_objects(&reopened, &[first_id, older_id, late_id, probe_id])
        .expect("reopened readback");
    for (id, bytes) in [first_id, older_id, late_id, probe_id].iter().zip(&read) {
        assert_eq!(ObjectId::for_bytes(bytes), *id);
    }
}
