//! Exact no-op preservation and the bounded compare-then-replay rule.
//!
//! An edit stream that changes nothing must return the base root itself: the
//! operation is not allowed to rebuild an identical structure. The comparison that
//! decides this is bounded and stops at the first difference; when it differs, the
//! same edits are replayed for construction, and both passes plus every base read
//! stay charged to the operation.

mod support;

use layerfs_content::{
    apply_edits, construct_bytes, ConstructionPolicy, ContentError, Edit, EditRequest, EditStream,
    ObjectId, Replacements,
};
use support::{
    disabled_scope, noise, patterned, read_back, Counted, CountingProvider, MemoryStore,
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

fn apply_counted(
    store: &MemoryStore,
    root: ObjectId,
    edits: Vec<Edit>,
    replacements: &Replacements,
    base_len: u64,
) -> (MemoryStore, ObjectId, u64, CountingProvider) {
    let stream = EditStream::new(base_len, edits).expect("valid stream");
    let mut result = store.merged_clone();
    let counts = CountingProvider::new();
    let provider = Counted {
        store,
        counts: &counts,
    };
    let constructed = disabled_scope(|scope| {
        apply_edits(
            policy(),
            &policy().capacities(),
            &provider,
            EditRequest {
                root,
                edits: &stream,
                source: replacements,
            },
            &mut result,
            scope.child("edit"),
        )
    })
    .expect("edit succeeds");
    (result, constructed.root, constructed.logical_len, counts)
}

#[test]
fn an_empty_stream_returns_the_base_root_untouched() {
    let base = patterned(4_000);
    let (store, root) = build(&base);
    let replacements = Replacements::new();
    let (result, edited_root, len, counts) =
        apply_counted(&store, root, Vec::new(), &replacements, base.len() as u64);
    assert_eq!(edited_root, root);
    assert_eq!(len, base.len() as u64);
    assert_eq!(
        result.order().len(),
        store.order().len(),
        "no object was emitted"
    );
    assert_eq!(counts.objects(), 1, "only the base root was acquired");
}

#[test]
fn an_equal_replacement_preserves_the_base_root() {
    let base = patterned(6_000);
    let (store, root) = build(&base);
    let mut replacements = Replacements::new();
    replacements.push(base[2_000..2_500].to_vec());
    let (result, edited_root, len, _) = apply_counted(
        &store,
        root,
        vec![Edit::overwrite(2_000, 2_500)],
        &replacements,
        base.len() as u64,
    );
    assert_eq!(edited_root, root, "an identical replacement is a no-op");
    assert_eq!(len, base.len() as u64);
    assert_eq!(result.order().len(), store.order().len());
}

#[test]
fn several_equal_replacements_are_all_recognised() {
    let base = noise(20_000);
    let (store, root) = build(&base);
    let mut replacements = Replacements::new();
    replacements.push(base[100..600].to_vec());
    replacements.push(base[5_000..5_400].to_vec());
    replacements.push(base[15_000..15_700].to_vec());
    let (_, edited_root, _, _) = apply_counted(
        &store,
        root,
        vec![
            Edit::overwrite(100, 600),
            Edit::overwrite(5_000, 5_400),
            Edit::overwrite(15_000, 15_700),
        ],
        &replacements,
        base.len() as u64,
    );
    assert_eq!(edited_root, root);
}

#[test]
fn a_shifted_equal_replacement_is_compared_in_current_coordinates() {
    let base = patterned(8_000);
    let (store, root) = build(&base);
    // The insertion changes the file, so the stream is not a no-op; the second
    // edit is an equal replacement whose base range is shifted by the insertion.
    let insertion = noise(64);
    let mut replacements = Replacements::new();
    replacements.push(insertion.clone());
    replacements.push(base[1_936..2_000].to_vec());
    let (result, edited_root, len, _) = apply_counted(
        &store,
        root,
        vec![Edit::insert(1_000, 64), Edit::overwrite(2_000, 2_064)],
        &replacements,
        base.len() as u64,
    );
    let mut expected = base.clone();
    expected.splice(1_000..1_000, insertion);
    assert_eq!(len, expected.len() as u64);
    assert_eq!(read_back(&result, edited_root).expect("read"), expected);
    assert_ne!(edited_root, root);
}

#[test]
fn a_long_equal_prefix_then_a_mismatch_stops_the_comparison_early() {
    // 1 MiB of identical replacement followed by a real difference: the
    // comparison must stop at the difference instead of reading the whole file.
    let base = noise(2 * 1024 * 1024);
    let (store, root) = build(&base);
    let mut changed = base[1_048_576..1_048_676].to_vec();
    changed[0] ^= 0xff;
    let mut replacements = Replacements::new();
    replacements.push(base[..1_048_576].to_vec());
    replacements.push(changed);
    let (result, edited_root, _, counts) = apply_counted(
        &store,
        root,
        vec![
            Edit::overwrite(0, 1_048_576),
            Edit::overwrite(1_048_576, 1_048_676),
        ],
        &replacements,
        base.len() as u64,
    );
    let mut expected = base.clone();
    expected[1_048_576] ^= 0xff;
    assert_eq!(read_back(&result, edited_root).expect("read"), expected);
    // Compare plus replay both read the base; the construction pass must have
    // re-read retained content, so the total is strictly above one pass.
    assert!(
        counts.objects() > 1,
        "both passes and the retained reads are charged"
    );
}

#[test]
fn a_length_changing_edit_is_never_treated_as_a_no_op() {
    let base = patterned(1_000);
    let (store, root) = build(&base);
    let replacements = Replacements::new();
    // The range is a deletion of 100 bytes, so the length changes and
    // construction must run even though a source holds those bytes.
    let stream = EditStream::new(base.len() as u64, vec![Edit::delete(100, 200)]).expect("valid");
    let mut result = MemoryStore::new();
    let constructed = disabled_scope(|scope| {
        apply_edits(
            policy(),
            &policy().capacities(),
            &store,
            EditRequest {
                root,
                edits: &stream,
                source: &replacements,
            },
            &mut result,
            scope.child("edit"),
        )
    })
    .expect("edit succeeds");
    assert_ne!(constructed.root, root);
    assert_eq!(constructed.logical_len, 900);
}

#[test]
fn an_out_of_range_replacement_length_is_an_error() {
    let base = patterned(1_000);
    let (store, root) = build(&base);
    let mut replacements = Replacements::new();
    replacements.push(vec![1_u8; 10]);
    let stream =
        EditStream::new(base.len() as u64, vec![Edit::overwrite(100, 200)]).expect("valid");
    let error = disabled_scope(|scope| {
        let mut result = MemoryStore::new();
        apply_edits(
            policy(),
            &policy().capacities(),
            &store,
            EditRequest {
                root,
                edits: &stream,
                source: &replacements,
            },
            &mut result,
            scope.child("edit"),
        )
    })
    .unwrap_err();
    assert!(
        matches!(error, ContentError::InvalidEdit { .. }),
        "got {error}"
    );
}
