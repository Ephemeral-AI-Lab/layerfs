//! Ordered multi-edit streams and the exact result they produce.
//!
//! The oracle is an independent model in this file: it applies the declared edits
//! to a plain byte vector, one at a time, in current-result coordinates. The
//! candidate must produce those exact bytes *and* the root of a fresh
//! construction of them, so a result that depended on the caller's segmentation
//! or on an internal reordering would not match.

mod support;

use layerfs_content::{
    apply_edits, construct_bytes, ConstructionPolicy, ContentError, Edit, EditRequest, EditSource,
    ObjectId,
};
use support::{
    disabled_scope, edits::Edits, edits::Parts, noise, patterned, read_back, MemoryStore,
};

/// Independent model: apply each edit to the running result, in order.
fn model_apply(base: &[u8], edits: &[Edit], source: &Parts) -> Vec<u8> {
    let mut result = base.to_vec();
    let mut previous_result_end = 0_u64;
    for (index, edit) in edits.iter().enumerate() {
        assert!(
            edit.start() >= previous_result_end,
            "model requires non-overlapping current-result coordinates"
        );
        let mut bytes = Vec::new();
        let mut offset = 0_u64;
        while offset < edit.replacement_len() {
            let mut window = vec![0_u8; (edit.replacement_len() - offset).min(4096) as usize];
            let read = source
                .read_at(index, offset, &mut window)
                .expect("replacement readable");
            assert!(read > 0);
            bytes.extend_from_slice(&window[..read]);
            offset += read as u64;
        }
        result.splice(edit.start() as usize..edit.end() as usize, bytes);
        previous_result_end = edit.start() + edit.replacement_len();
    }
    result
}

fn candidates(bytes: &[u8]) -> (MemoryStore, ObjectId) {
    let policy = ConstructionPolicy::frozen_default();
    let mut store = MemoryStore::new();
    let constructed = disabled_scope(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            bytes,
            &mut store,
            scope.child("content"),
        )
    })
    .expect("construction");
    (store, constructed.root)
}

fn apply(
    store: &MemoryStore,
    root: ObjectId,
    stream: &Edits,
    replacements: &Parts,
) -> (MemoryStore, ObjectId, u64) {
    let policy = ConstructionPolicy::frozen_default();
    let mut result = store.merged_clone();
    let constructed = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            store,
            EditRequest {
                root,
                edits: stream,
                source: replacements,
            },
            &mut result,
            scope.child("edit"),
        )
    })
    .expect("edit succeeds");
    (result, constructed.root, constructed.logical_len)
}

/// Checks bytes, length and root against the model and a fresh construction.
fn expect(name: &str, base: &[u8], edits: Vec<Edit>, replacements: &Parts) {
    let expected = model_apply(base, &edits, replacements);
    let (base_store, base_root) = candidates(base);
    let stream = Edits::new(base.len() as u64, edits).expect("valid stream");
    assert_eq!(
        stream.final_len(),
        expected.len() as u64,
        "{name}: model length"
    );
    let (result_store, root, len) = apply(&base_store, base_root, &stream, replacements);
    assert_eq!(len, expected.len() as u64, "{name}: length");
    let bytes = read_back(&result_store, root).expect("result reads back");
    assert_eq!(bytes, expected, "{name}: bytes");
    let (_, fresh_root) = candidates(&expected);
    assert_eq!(root, fresh_root, "{name}: root equals a fresh construction");
}

/// Deterministic replacement bytes for the declared length of every edit.
fn replacements_for(edits: &[Edit]) -> Parts {
    let mut source = Parts::new();
    for edit in edits {
        source.push(noise(edit.replacement_len() as usize));
    }
    source
}

#[test]
fn several_separated_edits_apply_in_order() {
    let base = patterned(6_000);
    let edits = vec![
        Edit::insert(500, 120),
        Edit::overwrite(2_120, 2_164),
        Edit::delete(4_064, 4_564),
    ];
    let replacements = replacements_for(&edits);
    expect("separated", &base, edits, &replacements);
}

#[test]
fn adjacent_edits_stay_separate_segments() {
    let base = patterned(2_048);
    let edits = vec![Edit::insert(100, 32), Edit::insert(132, 32)];
    let replacements = replacements_for(&edits);
    expect("adjacent", &base, edits, &replacements);
}

#[test]
fn delete_only_streams_are_monotonic_in_result_coordinates() {
    let base = patterned(3_000);
    let replacements = Parts::new();
    expect(
        "delete-only",
        &base,
        vec![Edit::delete(500, 700), Edit::delete(1_800, 2_300)],
        &replacements,
    );
}

#[test]
fn repeated_length_changes_accumulate() {
    let base = patterned(1_000);
    let edits = vec![
        Edit::insert(0, 100),
        Edit::delete(150, 250),
        Edit::insert(400, 250),
    ];
    let replacements = replacements_for(&edits);
    expect("accumulate", &base, edits, &replacements);
}

#[test]
fn many_small_edits_are_applied_in_one_pass() {
    let base = patterned(20_000);
    let mut edits = Vec::new();
    let mut position = 0_u64;
    for index in 0..200_u64 {
        position += 40;
        let start = position;
        if index % 3 == 0 {
            edits.push(Edit::insert(start, 7));
            position = start + 7;
        } else if index % 3 == 1 {
            edits.push(Edit::overwrite(start, start + 11));
            position = start + 11;
        } else {
            edits.push(Edit::delete(start, start + 5));
            position = start;
        }
    }
    let replacements = replacements_for(&edits);
    expect("many", &base, edits, &replacements);
}

#[test]
fn an_overlapping_stream_is_rejected_before_any_work() {
    let error = Edits::new(1_000, vec![Edit::delete(10, 100), Edit::delete(5, 20)]).unwrap_err();
    assert!(matches!(error, ContentError::InvalidEdit { .. }));
    let error = Edits::new(1_000, vec![Edit::insert(10, 40), Edit::overwrite(20, 30)]).unwrap_err();
    assert!(
        matches!(error, ContentError::InvalidEdit { .. }),
        "a range inside an earlier replacement is rejected, not merged"
    );
    // Adjacent edits are allowed and remain two segments.
    assert!(Edits::new(1_000, vec![Edit::delete(10, 100), Edit::delete(100, 200)]).is_ok());
    // The same result-coordinate start as the previous replacement is allowed.
    assert!(Edits::new(1_000, vec![Edit::insert(10, 40), Edit::insert(50, 5)]).is_ok());
}

#[test]
fn every_object_the_edit_emits_is_reachable_from_its_root() {
    let base = noise(200_000);
    let (base_store, base_root) = candidates(&base);
    let known: Vec<ObjectId> = base_store.order().iter().map(|(id, _)| *id).collect();
    let mut replacements = Parts::new();
    replacements.push(noise(1_000));
    let stream =
        Edits::new(base.len() as u64, vec![Edit::overwrite(70_000, 71_000)]).expect("valid");
    let (result_store, root, _) = apply(&base_store, base_root, &stream, &replacements);
    let bytes = read_back(&result_store, root).expect("reads back");
    assert_eq!(bytes.len(), base.len());
    for (id, _) in result_store.order() {
        if *id == root || known.contains(id) {
            continue;
        }
        assert!(
            support::reachable(&result_store, root, *id),
            "emitted object {id} is unreachable from the result root"
        );
    }
}
