//! Frozen model equivalence for known edits.
//!
//! The model in this file is an independent byte-vector implementation of the
//! declared semantics: it applies each edit to the running result in order. The
//! candidate must produce those bytes, and - wherever the representation is a whole
//! object - the root of a fresh construction of them. This target is the **model**
//! oracle; `edit_reference` is the separate sealed-reference tree oracle and is the
//! only one that can establish reference partition equivalence.

mod support;

use layerfs_content::{
    apply_edits, construct_bytes, ConstructionPolicy, Edit, EditRequest, EditSource, ObjectId,
};
use support::{
    disabled_scope, edits::Edits, edits::Parts, noise, patterned, read_back, MemoryStore,
};

fn policy() -> ConstructionPolicy {
    ConstructionPolicy::frozen_default()
}

fn build(bytes: &[u8]) -> (MemoryStore, ObjectId) {
    let mut store = MemoryStore::new();
    let constructed = disabled_scope(|scope| {
        construct_bytes(
            policy(),
            &policy().capacities(),
            bytes,
            &mut store,
            scope.child("content"),
        )
    })
    .expect("construction");
    (store, constructed.root)
}

/// Independent model: apply every edit to the running result, in order.
fn model_apply(base: &[u8], edits: &[Edit], source: &Parts) -> Vec<u8> {
    let mut result = base.to_vec();
    let mut previous_end = 0_u64;
    for (index, edit) in edits.iter().enumerate() {
        assert!(
            edit.start() >= previous_end,
            "the model requires non-overlapping current-result coordinates"
        );
        let mut bytes = Vec::new();
        let mut offset = 0_u64;
        while offset < edit.replacement_len() {
            let mut window = vec![0_u8; (edit.replacement_len() - offset).min(4096) as usize];
            let read = source
                .read_at(index, offset, &mut window)
                .expect("replacement readable");
            assert!(read > 0, "the replacement source ended early");
            bytes.extend_from_slice(&window[..read]);
            offset += read as u64;
        }
        result.splice(edit.start() as usize..edit.end() as usize, bytes);
        previous_end = edit.start() + edit.replacement_len();
    }
    result
}

fn apply(
    store: &MemoryStore,
    root: ObjectId,
    stream: &Edits,
    source: &Parts,
) -> (MemoryStore, ObjectId, u64) {
    let mut result = store.merged_clone();
    let constructed = disabled_scope(|scope| {
        apply_edits(
            policy(),
            &policy().capacities(),
            store,
            EditRequest {
                root,
                edits: stream,
                source,
            },
            &mut result,
            scope.child("edit"),
        )
    })
    .expect("edit succeeds");
    (result, constructed.root, constructed.logical_len)
}

/// Deterministic replacement bytes for the declared length of every edit.
fn replacements_for(edits: &[Edit]) -> Parts {
    let mut source = Parts::new();
    for edit in edits {
        source.push(noise(edit.replacement_len() as usize));
    }
    source
}

/// Bytes and length always; the fresh-construction root when `root_oracle` holds.
fn expect_model(name: &str, base: &[u8], edits: Vec<Edit>, root_oracle: bool) {
    let replacements = replacements_for(&edits);
    let expected = model_apply(base, &edits, &replacements);
    let (store, root) = build(base);
    let stream = Edits::new(base.len() as u64, edits).expect("valid stream");
    assert_eq!(
        stream.final_len(),
        expected.len() as u64,
        "{name}: accumulated length"
    );
    let (result, edited_root, len) = apply(&store, root, &stream, &replacements);
    assert_eq!(len, expected.len() as u64, "{name}: length");
    let bytes = read_back(&result, edited_root).expect("result reads back");
    assert_eq!(bytes, expected, "{name}: bytes");
    if root_oracle {
        let (_, fresh_root) = build(&expected);
        assert_eq!(
            edited_root, fresh_root,
            "{name}: root equals a fresh construction"
        );
    }
}

#[test]
fn a_small_file_matches_the_model_and_a_fresh_construction() {
    let base = patterned(9_000);
    expect_model(
        "small single",
        &base,
        vec![Edit::overwrite(4_000, 4_500)],
        true,
    );
    expect_model(
        "small batch",
        &base,
        vec![
            Edit::insert(100, 300),
            Edit::delete(2_000, 2_400),
            Edit::overwrite(5_000, 5_200),
        ],
        true,
    );
}

#[test]
fn an_append_at_end_of_file_matches_the_model() {
    // Regression: an append to a *chunked* file must reach the trailing replacement
    // even though the last base extent ends exactly at the append position.
    let base = noise(300_000);
    expect_model(
        "chunked append",
        &base,
        vec![Edit::insert(300_000, 5_000)],
        false,
    );
    let small = patterned(2_000);
    expect_model("small append", &small, vec![Edit::insert(2_000, 500)], true);
}

#[test]
fn a_chunked_update_matches_the_model() {
    let base = noise(400_000);
    expect_model(
        "chunked update",
        &base,
        vec![Edit::overwrite(150_000, 152_000)],
        false,
    );
    expect_model(
        "chunked mixed",
        &base,
        vec![
            Edit::insert(1_000, 9_000),
            Edit::delete(200_000, 210_000),
            Edit::overwrite(300_000, 301_000),
        ],
        false,
    );
}

#[test]
fn conversions_match_the_model_and_their_fresh_construction() {
    let cutoff = 131_072_usize;
    let small = patterned(cutoff - 1);
    let large = noise(cutoff + 50_000);
    // Small -> small and small -> large are constructed as whole objects or through
    // the complete builder, so a fresh construction is the oracle for both.
    expect_model(
        "small to small",
        &small,
        vec![Edit::overwrite(10, 20)],
        true,
    );
    expect_model(
        "small to large",
        &small,
        vec![Edit::insert(500, cutoff as u64)],
        true,
    );
    // Large -> small assembles retained ranges into one whole object.
    expect_model(
        "large to small",
        &large,
        vec![Edit::delete(1_000, large.len() as u64 - 1_000)],
        true,
    );
}

#[test]
fn a_deletion_that_merges_two_slices_stays_canonical() {
    // Deleting inside one chunk leaves two adjoining slices of the same payload,
    // which the canonical partition forbids: they must be coalesced.
    let base = noise(300_000);
    let edits = vec![Edit::delete(40_000, 40_100)];
    expect_model("coalesce", &base, edits, false);
}

#[test]
fn the_model_and_the_candidate_agree_on_degenerate_streams() {
    let base = patterned(5_000);
    expect_model("empty", &base, Vec::new(), true);
    expect_model("delete all", &base, vec![Edit::delete(0, 5_000)], true);
    // An equal replacement is exercised with the base bytes as the replacement.
    let replacements = {
        let mut source = Parts::new();
        source.push(base[1_000..2_000].to_vec());
        source
    };
    let stream = Edits::new(base.len() as u64, vec![Edit::overwrite(1_000, 2_000)]).expect("valid");
    let (store, root) = build(&base);
    let (result, edited_root, len) = apply(&store, root, &stream, &replacements);
    assert_eq!(edited_root, root, "an identical replacement is a no-op");
    assert_eq!(len, base.len() as u64);
    assert_eq!(read_back(&result, edited_root).expect("read"), base);
}
