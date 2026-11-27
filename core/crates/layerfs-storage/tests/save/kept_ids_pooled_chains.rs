//! Two Commit round-trip cuts (#768): a handle keeps its unused pack ids
//! (and, when it keeps ordinals, `KEPT_ORDINALS` unused ordinals) between
//! Saves and reserves only when a range runs out, dropping them after any
//! failed Save; and a pooled leaf's delta chain is read in a bounded number
//! of provider calls, not one per link. A keeping handle returns its kept
//! ordinals before an orderly drop; any other handle keeps none.
#[path = "../support/chain_provider.rs"]
mod chains;
#[path = "../support/memory_metadata.rs"]
mod metadata;

use chains::{copy, depth, ChainProvider};
use layerfs_content::{
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
    },
    AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance,
};
use layerfs_storage::{policy::KEPT_ORDINALS, port::PersistenceError, Storage};
use std::{collections::BTreeSet, sync::Arc};

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

/// `leaf(1, rewrites)`, naming the stored leaf it replaces.
fn version(rewrites: u64, replaced: Option<&FinalizedObject>) -> FinalizedObject {
    let object = leaf(1, rewrites);
    let Some(replaced) = replaced else {
        return object;
    };
    let mut named = AdvisoryPredecessors::new();
    named
        .push(replaced.id(), PredecessorProvenance::OriginalBase)
        .unwrap();
    object.with_predecessors(named)
}

fn save(storage: &Storage, objects: &[FinalizedObject]) -> layerfs_storage::StorageResult<()> {
    let save = storage.begin_save()?;
    for object in objects {
        save.accept(object.clone())?;
    }
    save.finish().map(drop)
}

/// The pack ids each registration published, in order.
fn published_packs(memory: &metadata::MemoryMetadata) -> Vec<i64> {
    let state = memory.state.lock().unwrap();
    state
        .registrations
        .iter()
        .flat_map(|batch| batch.packs.iter().map(|pack| pack.info().pack_id))
        .collect()
}

#[test]
fn a_kept_block_serves_later_saves_and_a_failed_save_drops_it() {
    let memory = Arc::new(metadata::MemoryMetadata::default());
    let storage = Storage::new(memory.clone()).unwrap().keeping_ordinals();
    let mut last = None;
    for rewrites in 0..3 {
        let object = version(rewrites, last.as_ref());
        save(&storage, &[object.clone()]).unwrap();
        last = Some(object);
    }
    // One reservation for three Saves; before #768 each Save reserved a
    // block of 4,096 pack ids and 16,384 ordinals and released all its
    // unused ordinals.
    let counts = storage.diagnostics();
    assert_eq!(counts.reserve, 1);
    assert_eq!(counts.initial_reservations, 1);
    assert_eq!(counts.reservation_refills, 0);
    let packs = published_packs(&memory);
    assert_eq!(packs, (1..=packs.len() as i64).collect::<Vec<_>>());
    // The first Save used ordinals 1..=8 and released the tail past the
    // KEPT_ORDINALS the handle keeps: the Store's counter is rewound to
    // 9 + KEPT_ORDINALS. The later Saves took their one new value each from the kept prefix and released
    // nothing.
    let kept = KEPT_ORDINALS as u64;
    let state = memory.state.lock().unwrap();
    assert_eq!(state.next_ordinal, 9 + kept);
    let releases: Vec<_> = state
        .registrations
        .iter()
        .filter_map(|batch| batch.release_ordinals)
        .collect();
    assert_eq!(
        releases,
        vec![(9 + kept as u32, 16_384 - 8 - KEPT_ORDINALS)]
    );
    assert!(state
        .values
        .keys()
        .all(|ordinal| u64::from(*ordinal) < 9 + kept));
    let counter = state.next_ordinal;
    drop(state);

    // A Save whose publication ends with an unknown outcome drops the block.
    let published = memory.state.lock().unwrap().registrations.len();
    memory.state.lock().unwrap().fail_register = Some(PersistenceError::Uncertain);
    let failed = version(3, last.as_ref());
    assert!(save(&storage, &[failed]).is_err());
    let object = version(4, last.as_ref());
    save(&storage, &[object.clone()]).unwrap();
    let counts = storage.diagnostics();
    assert_eq!(counts.reserve, 2, "the next Save reserves a new block");
    assert_eq!(counts.initial_reservations, 2);
    let after: Vec<i64> = published_packs(&memory)[packs.len()..].to_vec();
    assert!(!after.is_empty());
    assert!(
        after.iter().all(|id| *id > 4_096),
        "ids come from the new block: {after:?}"
    );
    let state = memory.state.lock().unwrap();
    let values: Vec<u32> = state.registrations[published..]
        .iter()
        .flat_map(|batch| batch.inode_values.iter().map(|row| row.ordinal))
        .collect();
    assert!(!values.is_empty());
    assert!(
        values.iter().all(|ordinal| u64::from(*ordinal) >= counter),
        "{values:?}"
    );
    drop(state);
    // A Save dropped before it finishes wrote nothing uncertain: its unused
    // rest stays on the handle and the next Save uses it, no reservation
    // (#768 review 6a; before, the block was dropped and the next Save
    // reserved a third one).
    let open = storage.begin_save().unwrap();
    open.accept(version(5, Some(&object))).unwrap();
    drop(open);
    save(&storage, &[version(6, Some(&object))]).unwrap();
    assert_eq!(storage.diagnostics().initial_reservations, 2);
    assert_eq!(storage.diagnostics().reserve, 2);
}

#[test]
fn a_save_that_fails_definitely_leaves_its_unused_rest_and_assigns_nothing_twice() {
    let memory = Arc::new(metadata::MemoryMetadata::default());
    let storage = Storage::new(memory.clone()).unwrap().keeping_ordinals();
    save(&storage, &[leaf(1, 0)]).unwrap();
    let kept = KEPT_ORDINALS as u64;
    let next = |memory: &metadata::MemoryMetadata| memory.state.lock().unwrap().next_ordinal;
    assert_eq!(next(&memory), ROWS + 1 + kept);
    // A definite refusal (Busy) of the next Save's publication: nothing
    // applied. Its values took ordinals ROWS + 1 ..= 2 * ROWS; the rest of
    // the kept block stays on the handle.
    memory.state.lock().unwrap().fail_register = Some(PersistenceError::Busy);
    assert!(save(&storage, &[leaf(100, 0)]).is_err());
    assert_eq!(next(&memory), ROWS + 1 + kept, "nothing applied");
    // The owner returns it: one release from the first unassigned ordinal.
    let before = calls(&memory, "register");
    storage.release_kept_ordinals().unwrap();
    assert_eq!(calls(&memory, "register"), before + 1);
    assert_eq!(
        next(&memory),
        2 * ROWS + 1,
        "rewound past the refused values"
    );
    // The refused Save's ordinals are never assigned again.
    save(&storage, &[leaf(200, 0)]).unwrap();
    let state = memory.state.lock().unwrap();
    let values: Vec<u32> = state.values.keys().copied().collect();
    assert_eq!(
        values[..ROWS as usize],
        (1..=ROWS as u32).collect::<Vec<_>>()
    );
    assert!(
        values[ROWS as usize..]
            .iter()
            .all(|ordinal| u64::from(*ordinal) > 2 * ROWS),
        "{values:?}"
    );
    drop(state);

    // A definite refusal of a Save's last publication, which also carried
    // the release of its unused tail: the whole unused block, from the first
    // unassigned ordinal to the reserved end, stays on the handle.
    let one_shot = Storage::new(memory.clone()).unwrap().keeping_ordinals();
    let reserved = next(&memory);
    memory.state.lock().unwrap().fail_register = Some(PersistenceError::Busy);
    assert!(save(&one_shot, &[leaf(300, 0)]).is_err());
    assert_eq!(next(&memory), reserved + 16_384, "the block stays reserved");
    one_shot.release_kept_ordinals().unwrap();
    assert_eq!(
        next(&memory),
        reserved + ROWS,
        "only the refused values stay"
    );
}

#[test]
fn the_kept_ordinals_are_bounded_and_a_larger_save_refills() {
    let memory = Arc::new(metadata::MemoryMetadata::default());
    // One Save per fresh handle, as one-shot callers make: a handle that
    // does not keep ordinals releases every unused one at finish, so the
    // next handle's values follow on with no gap (#768 verification finding
    // 6; with KEPT_ORDINALS kept and dropped, each gap was up to 128).
    for at in 0..3_u64 {
        save(
            &Storage::new(memory.clone()).unwrap(),
            &[leaf(1 + at * 100, 0)],
        )
        .unwrap();
    }
    let used = ordinals(&memory);
    assert_eq!(used, (1..=3 * ROWS as u32).collect::<Vec<_>>());
    assert_eq!(
        memory.state.lock().unwrap().next_ordinal,
        3 * ROWS + 1,
        "nothing left behind the last value"
    );
    // A kept handle whose next Save needs more new values than it kept
    // refills ordinals once, as before.
    let storage = Storage::new(memory.clone()).unwrap().keeping_ordinals();
    save(&storage, &[leaf(10_000, 0)]).unwrap();
    let wide: Vec<_> = (0..40_u64).map(|at| leaf(20_000 + at * ROWS, 0)).collect();
    assert!(wide.len() as u64 * ROWS > KEPT_ORDINALS as u64);
    save(&storage, &wide).unwrap();
    let counts = storage.diagnostics();
    assert_eq!(counts.initial_reservations, 1);
    assert_eq!(counts.reservation_refills, 1);
    assert_eq!(counts.ordinal_reservations, 2);
}

#[test]
fn ids_stay_unique_across_saves_and_across_two_handles_on_one_store() {
    let memory = Arc::new(metadata::MemoryMetadata::default());
    let handles = [
        Storage::new(memory.clone()).unwrap().keeping_ordinals(),
        Storage::new(memory.clone()).unwrap().keeping_ordinals(),
    ];
    let mut last: [Option<FinalizedObject>; 2] = [None, None];
    for round in 0..6_u64 {
        for (at, handle) in handles.iter().enumerate() {
            // Each handle's own file, so both publish new values each round.
            let object = leaf(1 + at as u64 * 1_000, round);
            let object = match &last[at] {
                Some(replaced) => {
                    let mut named = AdvisoryPredecessors::new();
                    named
                        .push(replaced.id(), PredecessorProvenance::OriginalBase)
                        .unwrap();
                    object.with_predecessors(named)
                }
                None => object,
            };
            save(handle, &[object.clone()]).unwrap();
            last[at] = Some(object);
        }
    }
    for handle in &handles {
        assert_eq!(handle.diagnostics().reserve, 1, "one block per handle");
    }
    let state = memory.state.lock().unwrap();
    let mut packs = BTreeSet::new();
    let mut ordinals = BTreeSet::new();
    for batch in &state.registrations {
        for pack in &batch.packs {
            assert!(packs.insert(pack.info().pack_id), "pack id reused");
        }
        for row in &batch.inode_values {
            assert!(ordinals.insert(row.ordinal), "ordinal reused");
        }
    }
    // The two handles' blocks are disjoint: one below 4,097, one above.
    assert!(packs.iter().any(|id| *id <= 4_096));
    assert!(packs.iter().any(|id| *id > 4_096));
    assert!(ordinals.len() as u64 >= 2 * (ROWS + 5));
}

#[test]
fn a_keeping_handle_returns_its_kept_ordinals_once_and_never_reuses_them() {
    let memory = Arc::new(metadata::MemoryMetadata::default());
    let storage = Storage::new(memory.clone()).unwrap().keeping_ordinals();
    save(&storage, &[leaf(1, 0)]).unwrap();
    let kept = KEPT_ORDINALS as u64;
    let next = |memory: &metadata::MemoryMetadata| memory.state.lock().unwrap().next_ordinal;
    // The finish rewound the counter to the end of the kept prefix.
    assert_eq!(next(&memory), ROWS + 1 + kept);
    let publications = |memory: &metadata::MemoryMetadata| calls(memory, "register");
    let before = publications(&memory);
    // The orderly release: one publication, which rewinds the counter to
    // right after the last value (before the fix a dropped handle left it
    // KEPT_ORDINALS past it).
    storage.release_kept_ordinals().unwrap();
    assert_eq!(publications(&memory), before + 1);
    assert_eq!(next(&memory), ROWS + 1);
    {
        let state = memory.state.lock().unwrap();
        let last = state.registrations.last().unwrap();
        assert_eq!(
            last.release_ordinals,
            Some((ROWS as u32 + 1, KEPT_ORDINALS))
        );
        assert!(last.objects.is_empty() && last.packs.is_empty());
    }
    // Returned once: a second call sends nothing, and the handle's next
    // Save refills ordinals (its kept pack ids still serve it).
    storage.release_kept_ordinals().unwrap();
    assert_eq!(publications(&memory), before + 1);
    let reserves = storage.diagnostics().reserve;
    save(&storage, &[leaf(1_000, 0)]).unwrap();
    let counts = storage.diagnostics();
    assert_eq!(counts.reserve, reserves + 1);
    assert_eq!(counts.reservation_refills, 1, "ordinals only");
    assert_eq!(
        ordinals(&memory),
        (1..=2 * ROWS as u32).collect::<Vec<_>>(),
        "the next values follow on"
    );

    // A refused release loses the kept ordinals: they leave the handle
    // first, the counter is not rewound, and nothing reuses them.
    let lost = next(&memory);
    memory.state.lock().unwrap().fail_register = Some(PersistenceError::Uncertain);
    assert!(storage.release_kept_ordinals().is_err());
    assert_eq!(next(&memory), lost);
    storage.release_kept_ordinals().unwrap();
    save(&storage, &[leaf(2_000, 0)]).unwrap();
    assert!(
        ordinals(&memory)[2 * ROWS as usize..]
            .iter()
            .all(|ordinal| u64::from(*ordinal) >= lost),
        "{:?}",
        ordinals(&memory)
    );

    // A one-shot handle keeps no ordinal: releasing sends nothing.
    let one_shot = Storage::new(memory.clone()).unwrap();
    save(&one_shot, &[leaf(3_000, 0)]).unwrap();
    let before = publications(&memory);
    one_shot.release_kept_ordinals().unwrap();
    assert_eq!(publications(&memory), before);
}

fn ordinals(memory: &metadata::MemoryMetadata) -> Vec<u32> {
    let state = memory.state.lock().unwrap();
    state.values.keys().copied().collect()
}

/// Nine Saves of one leaf, each on its own handle: a FULL leaf and eight
/// deltas, each in its own pack. Returns the Store and the last leaf.
fn pooled_history() -> (Arc<metadata::MemoryMetadata>, FinalizedObject) {
    let memory = Arc::new(metadata::MemoryMetadata::default());
    let mut last: Option<FinalizedObject> = None;
    for rewrites in 0..9 {
        let object = version(rewrites, last.as_ref());
        save(&Storage::new(memory.clone()).unwrap(), &[object.clone()]).unwrap();
        last = Some(object);
    }
    let last = last.unwrap();
    assert_eq!(depth(&memory, last.id()), 8, "fixture chain depth");
    (memory, last)
}

fn calls(memory: &metadata::MemoryMetadata, name: &str) -> usize {
    let calls = memory.calls.lock().unwrap();
    calls.iter().filter(|(call, _)| *call == name).count()
}

#[test]
fn a_pooled_chain_of_depth_eight_is_read_in_a_bounded_number_of_calls() {
    let (memory, last) = pooled_history();
    // Before #768 a Save's walk read one pack per link and located each base
    // alone: nine pack reads and nine locates for a chain of depth 8.
    let plain = copy(&memory);
    {
        let storage = Storage::new(plain.clone()).unwrap();
        let open = storage.begin_save().unwrap();
        assert_eq!(
            open.read_objects(&[last.id()]).unwrap(),
            vec![last.canonical().to_vec()]
        );
        assert_eq!(calls(&plain, "locate"), 9);
        assert!(calls(&plain, "read_scoped_pack") + calls(&plain, "read_packs") >= 9);
    }
    // Through a provider that answers chains and batches group reads (as
    // PostgreSQL does), a fresh handle's Save, and a reader, rebuild it with
    // two locates (the leaf with its chain; its base with the deeper bases),
    // one group read and one value lookup.
    for reader in [false, true] {
        let mut port = ChainProvider::new(copy(&memory));
        port.batched = true;
        let port = Arc::new(port);
        let storage = Storage::new(port.clone()).unwrap();
        storage.set_strict_hints(true);
        let got = if reader {
            storage.reader().unwrap().read_objects(&[last.id()])
        } else {
            let open = storage.begin_save().unwrap();
            let got = open.read_objects(&[last.id()]);
            drop(open);
            got
        };
        assert_eq!(got.unwrap(), vec![last.canonical().to_vec()]);
        assert!(port.calls("locate") <= 2, "reader={reader}");
        assert_eq!(port.calls("read_groups"), 1, "reader={reader}");
        assert_eq!(
            port.calls("read_scoped_pack") + port.calls("read_packs"),
            0,
            "reader={reader}"
        );
        assert_eq!(port.calls("inode_values"), 1, "reader={reader}");
        assert_eq!(storage.diagnostics().chain_hint_stale, 0);
    }
}

#[test]
fn a_kept_handle_reads_the_bodies_and_values_it_published_without_a_call() {
    let memory = Arc::new(metadata::MemoryMetadata::default());
    let storage = Storage::new(memory.clone()).unwrap();
    let mut last: Option<FinalizedObject> = None;
    for rewrites in 0..9 {
        let object = version(rewrites, last.as_ref());
        save(&storage, &[object.clone()]).unwrap();
        last = Some(object);
    }
    let last = last.unwrap();
    assert_eq!(depth(&memory, last.id()), 8);
    // The next Save on the same handle rebuilds the leaf from the packs and
    // values earlier Saves of this handle published: no pack read, no locate
    // and no value lookup (before #768 each Save forgot them).
    memory.calls.lock().unwrap().clear();
    let open = storage.begin_save().unwrap();
    assert_eq!(
        open.read_objects(&[last.id()]).unwrap(),
        vec![last.canonical().to_vec()]
    );
    drop(open);
    let log = memory.calls.lock().unwrap().clone();
    assert!(log.iter().all(|(call, _)| *call == "signatures"), "{log:?}");
    // That Save was dropped unfinished, so the next one reads the Store; it
    // takes the unused rest the dropped Save left on the handle (#768
    // review 6a), so it reserves nothing.
    memory.calls.lock().unwrap().clear();
    let open = storage.begin_save().unwrap();
    assert_eq!(
        open.read_objects(&[last.id()]).unwrap(),
        vec![last.canonical().to_vec()]
    );
    assert!(calls(&memory, "inode_values") == 1);
    assert!(calls(&memory, "reserve") == 0);
}
