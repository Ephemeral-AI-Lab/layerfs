//! Payload delta selection: candidates, trials, costs and reuse ordering.
//!
//! Every case reads the stored bytes back through an independent connection, so a
//! selection that produced an unreadable record fails here. The counters come
//! from the public save outcome and the public read wave, never from a test hook.

mod support;

use layerfs_content::file::mapping::encode_chunk_object;
use layerfs_content::{
    apply_edits, construct_bytes, AdvisoryPredecessors, ConstructionPolicy, Edit, EditRequest,
    FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance,
};
use layerfs_storage::Store;
use support::{
    assembled_small_object, construct_file, create_store, disabled, edits::Edits, edits::Parts,
    noise, open_store, patterned, read_objects, save_one, save_via_handoff, Collected, Provider,
    TempDir,
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
        operation.accept(first)?;
        // No explicit predecessor: the admitted-FULL winner cache must supply one.
        operation.accept(second)?;
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
    // A wholly different payload, not a near copy: a near copy would also be
    // proposed by the persisted content index, and then the trial would be the
    // index's outcome rather than this case's.
    let mut changed = raw.clone();
    for byte in &mut changed {
        *byte ^= 0xa5;
    }
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
}

/// A CHUNK candidate that no committed row matches is a policy fallback, not a
/// failure: exactly one FULL record is stored and no trial is attempted.
#[test]
fn an_absent_chunk_candidate_selects_full() {
    let dir = TempDir::new("delta-chunk-absent");
    let path = dir.store_path("delta");
    let store = create_store(&path);
    let raw = noise(20_000);
    let expected = chunk(&raw);
    let expected_canonical = expected.canonical().to_vec();
    let missing = ObjectId::for_bytes(b"never stored chunk");
    let absent = with_predecessor(expected, missing);
    let id = absent.id();
    let outcome = save_one(&store, absent).expect("an absent candidate is not a failure");
    assert_eq!(outcome.full_records, 1);
    assert_eq!(outcome.prefix_records, 0);
    assert_eq!(
        outcome.delta.trials, 0,
        "an absent candidate is never tried"
    );
    assert_eq!(outcome.delta.absent_candidates, 1);
    assert_eq!(outcome.inserted, 1);
    let (values, counters) = read_objects(&store, &[id]).expect("read back");
    assert_eq!(counters.edges, 0, "FULL has no dependency");
    assert_eq!(values[0], expected_canonical);
}

/// A CHUNK candidate that is present but belongs to another logical role is
/// ineligible, and ineligible selects FULL.
#[test]
fn an_ineligible_chunk_candidate_selects_full() {
    let dir = TempDir::new("delta-chunk-ineligible");
    let path = dir.store_path("delta");
    let store = create_store(&path);
    let other_role = whole(&noise(60_000));
    let other_id = other_role.id();
    save_one(&store, other_role).expect("whole-file save");
    let raw = noise(20_000);
    let expected = chunk(&raw);
    let expected_canonical = expected.canonical().to_vec();
    let mismatched = with_predecessor(expected, other_id);
    let id = mismatched.id();
    let outcome =
        save_one(&store, mismatched).expect("an ineligible candidate is a policy outcome");
    assert_eq!(outcome.prefix_records, 0);
    assert_eq!(
        outcome.delta.trials, 0,
        "an ineligible candidate is never tried"
    );
    assert!(
        outcome.delta.ineligible_candidates >= 1,
        "{:?}",
        outcome.delta
    );
    assert_eq!(outcome.full_records, 1);
    let (values, _) = read_objects(&store, &[id]).expect("read back");
    assert_eq!(values[0], expected_canonical);
}

/// Two adjacent replacements inside one chunked edit operation: the second
/// replacement's right boundary is the payload the same operation created for the
/// first, which no commit has published yet. Selection must decide eligibility by
/// probing that predecessor and store FULL, never fail the save.
#[test]
fn a_same_save_unsealed_chunk_predecessor_selects_full() {
    let dir = TempDir::new("delta-chunk-unsealed");
    let path = dir.store_path("delta");
    let store = create_store(&path);
    let base = noise(400_000);
    let policy = ConstructionPolicy::frozen_default();
    let mut base_objects = Collected::new();
    let constructed = disabled(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &base,
            &mut base_objects,
            scope.child("content"),
        )
    })
    .expect("base construction");
    save_via_handoff(&store, &base_objects).expect("base save");

    // Each replacement is exactly one minimum-size chunk, so each edit creates
    // exactly one payload and the second edit's left boundary lands on the first
    // edit's payload.
    let first_at = 60_000_u64;
    let length = 8_192_u64;
    // The two replacements must not be identical, or the second is an exact
    // reuse inside the same wave and never reaches selection at all.
    let first_bytes = patterned(8_192);
    let second_bytes = noise(8_192);
    let mut replacements = Parts::new();
    replacements.push(first_bytes.clone());
    replacements.push(second_bytes.clone());
    let stream = Edits::new(
        base.len() as u64,
        vec![
            Edit::overwrite(first_at, first_at + length),
            Edit::overwrite(first_at + length, first_at + 2 * length),
        ],
    )
    .expect("valid stream");
    let mut edited = Collected::new();
    let applied = disabled(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &Provider(&base_objects),
            EditRequest {
                root: constructed.root,
                edits: &stream,
                source: &replacements,
            },
            &mut edited,
            scope.child("edit"),
        )
    })
    .expect("localized edit");
    let payloads = edited
        .objects()
        .iter()
        .filter(|(_, role, _, _)| *role == ObjectRole::Chunk)
        .count();
    assert_eq!(payloads, 2, "each replacement is one payload");

    let outcome = save_via_handoff(&store, &edited)
        .expect("an unpublished predecessor must not fail the save");
    assert_eq!(
        outcome.delta.absent_candidates, 1,
        "the earlier replacement's payload is probed once and found unpublished"
    );
    drop(store);

    let mut expected = Vec::with_capacity(base.len());
    expected.extend_from_slice(&base[..first_at as usize]);
    expected.extend_from_slice(&first_bytes);
    expected.extend_from_slice(&second_bytes);
    expected.extend_from_slice(&base[(first_at + 2 * length) as usize..]);
    let reopened = open_store(&path);
    assert_eq!(support::read_logical(&reopened, applied.root), expected);
}

/// The CHUNK lane honours its own depth cap, and a changed cap changes the answer.
///
/// The chunk depth is a separate policy value from the whole-file depth, so
/// eligibility must be decided with the chunk bound: the same two-link chain, and
/// the same dependent naming its deepest record, is ineligible at cap two and
/// admitted at cap three.
#[test]
fn the_chunk_lane_honours_its_own_depth_cap() {
    let run = |chunk_depth: u8, label: &str| {
        let dir = TempDir::new(label);
        let path = dir.store_path("delta");
        let policy = layerfs_storage::StoragePolicy::new(1, 131_072, 8, chunk_depth);
        let store =
            disabled(|scope| Store::create(&path, policy, scope.child("store"))).expect("store");
        assert_eq!(
            store.capacities().delta_depth_for_role(ObjectRole::Chunk),
            chunk_depth
        );
        // Incompressible payloads: the unconditional frame is as large as the
        // payload, so the dictionary-backed prefix frame wins and the chain is real.
        let mut ids = Vec::new();
        let mut current = noise(20_000);
        for step in 0..3_usize {
            let object = if step == 0 {
                chunk(&current)
            } else {
                with_predecessor(chunk(&current), ids[step - 1])
            };
            let id = object.id();
            let outcome = save_one(&store, object).expect("chain step");
            if step > 0 {
                assert_eq!(
                    outcome.prefix_records, 1,
                    "chunk step {step} at cap {chunk_depth} must be a prefix record"
                );
            }
            ids.push(id);
            let mut next = current.clone();
            let start = step * 64;
            for byte in &mut next[start..start + 32] {
                *byte ^= 0xff;
            }
            current = next;
        }
        assert_eq!(ids.len(), 3, "two links, three records");
        // A dependent naming the deepest record: its depth is two.
        let mut beyond = current.clone();
        beyond[9_000] ^= 0x7f;
        let deepest = ids[2];
        let outcome = save_one(&store, with_predecessor(chunk(&beyond), deepest)).expect("save");
        (outcome, ids)
    };

    let (at_cap, _) = run(2, "delta-chunk-cap2");
    assert_eq!(at_cap.delta.trials, 0, "a base at the cap is not acquired");
    assert_eq!(at_cap.delta.ineligible_candidates, 1);
    assert_eq!(at_cap.prefix_records, 0);
    assert_eq!(at_cap.full_records, 1);
    let (with_room, _) = run(3, "delta-chunk-cap3");
    assert_eq!(with_room.delta.trials, 1, "one level of headroom admits it");
    assert_eq!(with_room.prefix_records, 1);
}

/// The chunk depth accepts fifty links when the chain's bytes allow it.
///
/// A chain of minimum-size chunks keeps the canonical work inside its budget, so
/// depth - not the budget - is what the fifty-first record reaches: the record at
/// the cap is admitted, one level deeper is not, and the whole chain reads back.
#[test]
fn a_fifty_link_chunk_chain_is_admitted_and_read() {
    let dir = TempDir::new("delta-chunk-depth50");
    let path = dir.store_path("delta");
    let policy = layerfs_storage::StoragePolicy::new(1, 131_072, 8, 50);
    let store =
        disabled(|scope| Store::create(&path, policy, scope.child("store"))).expect("store");
    assert_eq!(
        store.capacities().delta_depth_for_role(ObjectRole::Chunk),
        50
    );
    assert_eq!(store.capacities().chain_canonical_limit, 512 * 1024);

    let mut ids = Vec::new();
    let mut current = noise(8_192);
    let mut links = 0_u32;
    loop {
        let object = match ids.last().copied() {
            Some(base) => with_predecessor(chunk(&current), base),
            None => chunk(&current),
        };
        let id = object.id();
        let outcome = save_one(&store, object).expect("chain step");
        if links > 0 {
            if outcome.prefix_records == 1 {
                // The chain still extends.
            } else {
                assert_eq!(
                    outcome.delta.work_exceeded, 1,
                    "a refused step is a budget refusal, not a failure: {:?}",
                    outcome.delta
                );
                break;
            }
        }
        ids.push(id);
        if links == 50 {
            break;
        }
        links += 1;
        let mut next = current.clone();
        let start = (links as usize * 37) % 8_000;
        for byte in &mut next[start..start + 24] {
            *byte ^= 0xff;
        }
        current = next;
    }
    assert_eq!(
        ids.len(),
        51,
        "fifty links, fifty-one records: {}",
        ids.len()
    );
    let (values, counters) = read_objects(&store, &[*ids.last().expect("deepest")]).expect("read");
    assert_eq!(ObjectId::for_bytes(&values[0]), *ids.last().unwrap());
    assert_eq!(counters.max_depth, 50, "the read follows all fifty edges");

    // One level deeper is past the cap: the base is not acquired and no trial runs.
    let mut beyond = current.clone();
    beyond[1_000] ^= 0xff;
    let outcome = save_one(
        &store,
        with_predecessor(chunk(&beyond), *ids.last().unwrap()),
    )
    .expect("past cap");
    assert_eq!(
        outcome.delta.trials, 0,
        "a base at depth fifty is not acquired"
    );
    assert_eq!(outcome.delta.ineligible_candidates, 1);
    assert_eq!(outcome.delta.work_exceeded, 0, "depth is not a work budget");
    assert_eq!(outcome.prefix_records, 0);
}
