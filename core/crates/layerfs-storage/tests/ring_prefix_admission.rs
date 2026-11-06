//! A whole file whose base the similarity ring proposed from an earlier Save,
//! and which is stored as a PREFIX record, joins the ring itself (#88), so the
//! next similar file at a new path finds the newest version rather than the
//! first. A proposal of the same Save's own objects (an Init of many similar
//! files) admits nothing, so such a Save proposes, seals and records exactly
//! what it did before: only FULL winners. An object whose declared base was
//! used never enters the ring. Chains grown this way stay
//! bounded: a proposal whose chain cannot take the new file within the work
//! budgets is re-based onto that chain's root (#89), never stored FULL.
#[path = "support/memory_metadata.rs"]
mod metadata;
mod support;

use layerfs_content::{AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::{Storage, StoragePolicy, WriteOutcome};
use metadata::MemoryMetadata;
use std::sync::Arc;

/// `len` bytes of noise with the byte at `version * 7` changed: every version
/// differs from every other in a byte or two.
fn version(len: usize, version: usize) -> Vec<u8> {
    let mut raw = support::noise(len);
    raw[version * 7] ^= 0x5a;
    raw
}

fn object(raw: &[u8], base: Option<ObjectId>) -> FinalizedObject {
    let object =
        FinalizedObject::new(ObjectRole::WholeFile, support::assembled_small_object(raw)).unwrap();
    match base {
        Some(base) => object.with_predecessors(AdvisoryPredecessors::explicit(base).unwrap()),
        None => object,
    }
}

fn store() -> Arc<MemoryMetadata> {
    let metadata = Arc::new(MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = StoragePolicy::default();
    metadata
}

/// Saves each object in its own Save on a fresh handle, so every selection
/// reads the ring back from the Store.
fn save_each(metadata: &Arc<MemoryMetadata>, objects: &[FinalizedObject]) -> Vec<WriteOutcome> {
    objects
        .iter()
        .map(|object| {
            let storage = Storage::new(metadata.clone()).unwrap();
            let save = storage.begin_save().unwrap();
            save.accept(object.clone()).unwrap();
            save.finish().unwrap()
        })
        .collect()
}

fn depth(outcome: &WriteOutcome) -> usize {
    let depths = outcome.delta.whole_file_depths;
    assert_eq!(depths.iter().sum::<u64>(), 1, "{outcome:?}");
    depths.iter().position(|n| *n == 1).unwrap()
}

fn ring_ids(metadata: &MemoryMetadata) -> Vec<ObjectId> {
    let state = metadata.state.lock().unwrap();
    let mut rows: Vec<_> = state.signatures.values().collect();
    rows.sort_by_key(|row| row.stamp);
    rows.iter().map(|row| row.object_id).collect()
}

fn assert_reads_back(metadata: &Arc<MemoryMetadata>, objects: &[FinalizedObject]) {
    let storage = Storage::new(metadata.clone()).unwrap();
    let reader = storage.reader().unwrap();
    for object in objects {
        assert_eq!(
            reader.read_objects(&[object.id()]).unwrap(),
            vec![object.canonical().to_vec()]
        );
    }
}

#[test]
fn a_third_version_at_a_new_path_deltas_against_the_second_not_the_first() {
    // No version declares a predecessor: each is a new file at a new path.
    let metadata = store();
    let objects: Vec<_> = (0..3)
        .map(|v| object(&version(8 * 1024, v), None))
        .collect();
    let outcomes = save_each(&metadata, &objects);
    // v1 is FULL; v2 is a delta on v1 found by the ring; v3 is found by the ring
    // too and sits at depth two, so its base is v2 (depth one), not v1.
    assert_eq!(outcomes.iter().map(depth).collect::<Vec<_>>(), [0, 1, 2]);
    for outcome in &outcomes[1..] {
        assert_eq!(outcome.prefix_records, 1, "{outcome:?}");
        assert_eq!(outcome.delta.trials, 1, "{outcome:?}");
    }
    let ids: Vec<_> = objects.iter().map(FinalizedObject::id).collect();
    assert_eq!(
        ring_ids(&metadata),
        ids,
        "every ring-based winner joins the ring"
    );
    assert_reads_back(&metadata, &objects);
}

#[test]
fn a_file_stored_against_its_declared_base_does_not_enter_the_ring() {
    let metadata = store();
    let first = object(&version(8 * 1024, 0), None);
    let second = object(&version(8 * 1024, 1), Some(first.id()));
    let outcomes = save_each(&metadata, &[first.clone(), second.clone()]);
    assert_eq!(outcomes.iter().map(depth).collect::<Vec<_>>(), [0, 1]);
    assert_eq!(ring_ids(&metadata), vec![first.id()]);
    assert_reads_back(&metadata, &[first, second]);
}

#[test]
fn a_ring_chain_over_the_budget_re_bases_onto_its_root_instead_of_storing_full() {
    // 100 KiB versions at new paths: the 512 KiB canonical budget takes a chain
    // of five. The sixth's proposal, the newest version, cannot take it, so the
    // one trial runs against that chain's root; the seventh chains on the sixth.
    let metadata = store();
    let objects: Vec<_> = (0..7)
        .map(|v| object(&version(100 * 1024, v), None))
        .collect();
    let outcomes = save_each(&metadata, &objects);
    assert_eq!(
        outcomes.iter().map(depth).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 1, 2]
    );
    for (index, outcome) in outcomes.iter().enumerate() {
        assert_eq!(
            outcome.full_records,
            u64::from(index == 0),
            "{index}: {outcome:?}"
        );
        assert_eq!(outcome.delta.work_exceeded, 0, "{index}: {outcome:?}");
        assert_eq!(
            outcome.delta.rebased,
            u64::from(index == 5),
            "{index}: {outcome:?}"
        );
        assert_eq!(
            outcome.delta.trials,
            u64::from(index > 0),
            "{index}: {outcome:?}"
        );
    }
    assert_reads_back(&metadata, &objects);
}

#[test]
fn an_init_of_similar_files_in_one_save_admits_only_full_winners() {
    // Thirty similar files at new paths, all in one Save, as a `project init`
    // offers them: every ring proposal is an object of this Save.
    let metadata = store();
    let objects: Vec<_> = (0..30)
        .map(|v| object(&version(8 * 1024, v), None))
        .collect();
    let storage = Storage::new(metadata.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    for object in &objects {
        save.accept(object.clone()).unwrap();
    }
    let outcome = save.finish().unwrap();
    // The first is FULL and every other one is a delta on it: no PREFIX winner
    // ever became a proposal, so nothing pending beyond the FULL winner was
    // proposed or sealed early, exactly as before ring admission existed.
    let depths = outcome.delta.whole_file_depths;
    assert_eq!((depths[0], depths[1]), (1, 29), "{outcome:?}");
    assert_eq!(depths[2..].iter().sum::<u64>(), 0, "{outcome:?}");
    assert_eq!(outcome.full_records, 1, "{outcome:?}");
    // The ring holds the FULL winner only.
    assert_eq!(ring_ids(&metadata), vec![objects[0].id()]);
    assert_reads_back(&metadata, &objects);

    // The next Save, at a new path, proposes from the earlier Save and is
    // admitted, so lineage across Saves still advances.
    let next = object(&version(8 * 1024, 30), None);
    let outcome = save_each(&metadata, std::slice::from_ref(&next)).remove(0);
    assert_eq!(depth(&outcome), 1, "{outcome:?}");
    assert_eq!(ring_ids(&metadata), vec![objects[0].id(), next.id()]);
}

#[test]
fn a_winner_admitted_in_a_save_is_never_proposed_back_to_that_save() {
    // Two similar files A and B, each with a new version per Commit at a new
    // path. B's first version is a delta on A's, inside one Save, so B starts
    // with no ring entry of its own.
    let a = |v: usize| version(8 * 1024, v);
    let b = |v: usize| {
        let mut raw = version(8 * 1024, v);
        raw[6_000 + v] ^= 0x33;
        raw
    };
    let metadata = store();
    let first = [object(&a(0), None), object(&b(0), None)];
    let storage = Storage::new(metadata.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    for object in &first {
        save.accept(object.clone()).unwrap();
    }
    save.finish().unwrap();
    assert_eq!(ring_ids(&metadata), vec![first[0].id()]);
    // Second Commit: A's next version is admitted (its proposal is from the
    // first Save). B's proposal would be that pending admission; it is answered
    // by A's first version instead, so both are depth one and the Save seals no
    // early pack.
    let second = [object(&a(1), None), object(&b(1), None)];
    let storage = Storage::new(metadata.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    for object in &second {
        save.accept(object.clone()).unwrap();
    }
    let outcome = save.finish().unwrap();
    let depths = outcome.delta.whole_file_depths;
    assert_eq!(
        (depths[1], depths[2..].iter().sum::<u64>()),
        (2, 0),
        "{outcome:?}"
    );
    assert_eq!(outcome.packs, 1, "{outcome:?}");
    // Both second versions joined the ring, so B's own lineage starts here.
    assert_eq!(
        ring_ids(&metadata),
        vec![first[0].id(), second[0].id(), second[1].id()]
    );
    let mut all = first.to_vec();
    all.extend(second);
    assert_reads_back(&metadata, &all);
}
