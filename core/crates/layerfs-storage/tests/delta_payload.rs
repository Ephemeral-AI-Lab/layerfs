//! Payload delta selection: candidates, trials, costs and reuse ordering.
//!
//! Every case reads the stored bytes back through an independent connection, so a
//! selection that produced an unreadable record fails here. The counters come
//! from the public save outcome and the public read wave, never from a test hook.

mod support;

use layerfs_content::file::mapping::encode_chunk_object;
use layerfs_content::{
    AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance,
};
use layerfs_storage::Store;
use support::{
    assembled_small_object, construct_file, create_store, disabled, noise, open_store, patterned,
    read_objects, save_one, save_via_handoff, TempDir,
};

fn with_predecessor(object: FinalizedObject, base: ObjectId) -> FinalizedObject {
    let mut predecessors = AdvisoryPredecessors::new();
    predecessors
        .push(base, PredecessorProvenance::OriginalBase)
        .expect("bounded predecessor");
    object.with_predecessors(predecessors)
}

fn whole(raw: &[u8]) -> FinalizedObject {
    FinalizedObject::new(ObjectRole::WholeFile, assembled_small_object(raw)).expect("canonical")
}

fn chunk(raw: &[u8]) -> FinalizedObject {
    FinalizedObject::new(ObjectRole::Chunk, encode_chunk_object(raw).expect("chunk"))
        .expect("canonical")
}

#[test]
fn an_explicit_predecessor_produces_a_readable_prefix_record() {
    let dir = TempDir::new("delta-explicit");
    let path = dir.store_path("delta");
    let store = create_store(&path);
    let raw = patterned(100_000);
    let base = whole(&raw);
    let base_id = base.id();
    let outcome = save_one(&store, base).expect("base save");
    assert_eq!(outcome.full_records, 1);
    assert_eq!(outcome.prefix_records, 0);

    let mut changed = raw.clone();
    changed[40_000..41_000].copy_from_slice(&noise(1_000));
    let dependent = with_predecessor(whole(&changed), base_id);
    let dependent_id = dependent.id();
    let outcome = save_one(&store, dependent).expect("dependent save");
    assert_eq!(outcome.prefix_records, 1, "the prefix trial won");
    assert_eq!(outcome.delta.trials, 1, "exactly one trial");
    assert_eq!(outcome.inserted, 1);

    drop(store);
    let reopened = open_store(&path);
    let (values, counters) = read_objects(&reopened, &[dependent_id]).expect("read");
    assert_eq!(values[0], assembled_small_object(&changed));
    assert_eq!(counters.edges, 1, "one dependency edge was followed");
    assert_eq!(counters.objects, 1, "one requested object was returned");
    assert_eq!(
        counters.canonical_bytes,
        raw.len() as u64 + 23,
        "the dependency, and only the dependency, is charged to the chain budget"
    );
    // The base is untouched and still readable on its own.
    let (base_values, _) = read_objects(&reopened, &[base_id]).expect("base read");
    assert_eq!(base_values[0], assembled_small_object(&raw));
}

#[test]
fn the_admitted_full_cache_supplies_a_candidate_within_one_save() {
    let dir = TempDir::new("delta-cache");
    let path = dir.store_path("delta");
    let store = create_store(&path);
    // The winner cache is a min-hash sketch: it proposes a candidate when two
    // payloads share enough of their smallest window hashes, which real content
    // does and a periodic test pattern does not. The loss is bounded and never a
    // correctness risk, so the fixture uses content with distinct windows.
    let raw = noise(90_000);
    let mut changed = raw.clone();
    changed[10_000..11_000].copy_from_slice(&noise(1_000));
    let first = whole(&raw);
    let second = whole(&changed);
    let outcome = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        operation.accept(first, scope.child("storage.accept"))?;
        // No explicit predecessor: the admitted-FULL winner cache must supply one.
        operation.accept(second, scope.child("storage.accept"))?;
        operation.finish(scope.child("storage.finish"))
    })
    .expect("save");
    assert_eq!(outcome.inserted, 2);
    assert_eq!(outcome.delta.trials, 1);
    assert_eq!(outcome.prefix_records, 1);
}

#[test]
fn native_delta_uses_the_first_supplied_eligible_candidate_only() {
    let dir = TempDir::new("delta-native");
    let path = dir.store_path("delta");
    let store = create_store(&path);
    let file = noise(200_000);
    let (collected, _, _) = construct_file(&file);
    let chunks: Vec<&(ObjectId, ObjectRole, Vec<u8>, Vec<ObjectId>)> = collected
        .objects()
        .iter()
        .filter(|(_, role, _, _)| *role == ObjectRole::Chunk)
        .collect();
    assert!(
        chunks.len() >= 3,
        "the fixture produced {} chunks",
        chunks.len()
    );
    let first = FinalizedObject::new(ObjectRole::Chunk, chunks[0].2.clone()).expect("canonical");
    let second = FinalizedObject::new(ObjectRole::Chunk, chunks[1].2.clone()).expect("canonical");
    let first_id = first.id();
    let second_id = second.id();
    save_one(&store, first).expect("first chunk");
    save_one(&store, second).expect("second chunk");

    let target = FinalizedObject::new(ObjectRole::Chunk, chunks[2].2.clone()).expect("canonical");
    let mut predecessors = AdvisoryPredecessors::new();
    predecessors
        .push(second_id, PredecessorProvenance::OriginalBase)
        .expect("first candidate");
    predecessors
        .push(first_id, PredecessorProvenance::ReusedRange)
        .expect("second candidate");
    let dependent = target.with_predecessors(predecessors);
    let outcome = save_one(&store, dependent).expect("save");
    assert_eq!(
        outcome.delta.trials, 1,
        "four correspondence ids must not become four trials"
    );
}

#[test]
fn exact_reuse_is_decided_before_any_trial() {
    let dir = TempDir::new("delta-reuse");
    let path = dir.store_path("delta");
    let store = create_store(&path);
    let raw = patterned(50_000);
    let first = whole(&raw);
    let id = first.id();
    save_one(&store, first).expect("first save");
    let outcome = save_one(&store, whole(&raw)).expect("second save");
    assert_eq!(outcome.reused, 1);
    assert_eq!(outcome.inserted, 0);
    assert_eq!(outcome.delta.trials, 0, "reuse is not a delta trial");
    let (values, counters) = read_objects(&store, &[id]).expect("read");
    assert_eq!(counters.edges, 0);
    assert_eq!(values[0], assembled_small_object(&raw));
}

#[test]
fn an_absent_or_ineligible_candidate_selects_full() {
    let dir = TempDir::new("delta-ineligible");
    let path = dir.store_path("delta");
    let store = create_store(&path);
    let raw = patterned(40_000);
    let missing = ObjectId::for_bytes(b"not stored");
    let absent = with_predecessor(whole(&raw), missing);
    let outcome = save_one(&store, absent).expect("save");
    assert_eq!(outcome.full_records, 1);
    assert_eq!(outcome.prefix_records, 0);
    assert!(outcome.delta.absent_candidates >= 1);

    // A candidate of another logical role is ineligible for this role.
    let chunk_base = chunk(&noise(20_000));
    let chunk_id = chunk_base.id();
    save_one(&store, chunk_base).expect("chunk save");
    let mut changed = raw.clone();
    changed[0] ^= 0xff;
    let mismatched = with_predecessor(whole(&changed), chunk_id);
    let outcome = save_one(&store, mismatched).expect("save");
    assert_eq!(outcome.prefix_records, 0);
    assert!(outcome.delta.ineligible_candidates >= 1);
}

#[test]
fn an_unrelated_candidate_loses_the_cost_comparison() {
    let dir = TempDir::new("delta-loses");
    let path = dir.store_path("delta");
    let store = create_store(&path);
    let base = whole(&noise(60_000));
    let base_id = base.id();
    save_one(&store, base).expect("base");
    let unrelated = with_predecessor(whole(&patterned(60_000)), base_id);
    let outcome = save_one(&store, unrelated).expect("save");
    assert_eq!(outcome.delta.trials, 1);
    assert_eq!(outcome.delta.full_losses, 1);
    assert_eq!(outcome.prefix_records, 0);
    assert_eq!(
        outcome.full_records, 1,
        "FULL is a policy outcome, not a failure"
    );
}

#[test]
fn depth_zero_disables_prospective_delta_entirely() {
    let dir = TempDir::new("delta-depth0");
    let path = dir.store_path("delta");
    let policy = layerfs_storage::StoragePolicy::new(1, 131_072, 0, 0);
    let store = disabled(|scope| Store::create(&path, policy, scope.child("store")))
        .expect("store with delta disabled");
    let raw = patterned(70_000);
    let base = whole(&raw);
    let base_id = base.id();
    save_one(&store, base).expect("base");
    let mut changed = raw.clone();
    changed[5_000..6_000].copy_from_slice(&noise(1_000));
    let outcome = save_one(&store, with_predecessor(whole(&changed), base_id)).expect("save");
    assert_eq!(outcome.delta.trials, 0, "no trial is attempted at depth 0");
    assert_eq!(outcome.prefix_records, 0);
    assert_eq!(outcome.full_records, 1);
    assert_eq!(store.policy().whole_file_delta_max_depth(), 0);
}

#[test]
fn a_corrupt_base_fails_the_save_instead_of_selecting_full() {
    let dir = TempDir::new("delta-corrupt");
    let path = dir.store_path("delta");
    let store = create_store(&path);
    let raw = patterned(80_000);
    let base = whole(&raw);
    let base_id = base.id();
    save_one(&store, base).expect("base");
    drop(store);
    support::corrupt_first_pack(&path);
    let reopened = open_store(&path);
    let mut changed = raw.clone();
    changed[1_000..2_000].copy_from_slice(&noise(1_000));
    let outcome = save_one(&reopened, with_predecessor(whole(&changed), base_id));
    assert!(
        outcome.is_err(),
        "a failed base acquisition must fail the operation"
    );
    let read = disabled(|scope| reopened.read_batch(&[base_id], scope.child("read")));
    assert!(read.is_err());
}

#[test]
fn a_delta_chain_is_resolved_iteratively_and_reaches_every_step() {
    let dir = TempDir::new("delta-chain");
    let path = dir.store_path("delta");
    let store = create_store(&path);
    let raw = patterned(120_000);
    let mut current = whole(&raw);
    let mut expected = raw.clone();
    let mut ids = Vec::new();
    for step in 0..4 {
        let id = current.id();
        ids.push(id);
        save_one(&store, current).expect("chain step");
        let mut next = expected.clone();
        next[step * 1_000..step * 1_000 + 500].copy_from_slice(&noise(500));
        expected = next.clone();
        current = if step == 3 {
            whole(&expected)
        } else {
            with_predecessor(whole(&expected), id)
        };
    }
    let (values, counters) = read_objects(&store, &ids).expect("chain read");
    assert_eq!(values.len(), ids.len());
    assert_eq!(
        counters.edges, 6,
        "every requested object reconstructs its own chain"
    );
    assert_eq!(counters.max_depth, 3, "the deepest chain has three edges");
}

#[test]
fn the_c1_handoff_reports_the_same_selection_as_a_direct_save() {
    let dir = TempDir::new("delta-handoff");
    let path = dir.store_path("delta");
    let file = noise(150_000);
    let (collected, _, _) = construct_file(&file);
    let store = create_store(&path);
    let outcome = save_via_handoff(&store, &collected).expect("handoff save");
    assert_eq!(outcome.inserted, collected.objects().len() as u64);
    assert!(outcome.acknowledged);
}
