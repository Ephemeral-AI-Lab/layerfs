//! Representation transitions at every accepted construction cutoff.
//!
//! The cutoff is configurable, so the three representative values are exercised
//! directly: the default 128 KiB, 256 KiB and 1 MiB. For each one the exact
//! boundary lengths, all four conversion directions and both complete-length
//! entry points are checked against an independent fresh construction.

mod support;

use layerfs_content::file::classify;
use layerfs_content::{
    apply_edits, construct_bytes, construct_stream, ConstructionPolicy, ContentError, Edit,
    EditRequest, EditStream, FileContent, ObjectId, Replacements,
};
use support::{disabled_scope, noise, patterned, read_back, MemoryStore};

/// Cutoffs this slice accepts and the tests exercise.
const CUTOFFS: [u64; 3] = [131_072, 262_144, 1_048_576];

fn policy_for(cutoff: u64) -> ConstructionPolicy {
    ConstructionPolicy::new(cutoff, 8, 4)
        .validated()
        .expect("accepted cutoff")
}

fn build(policy: ConstructionPolicy, bytes: &[u8]) -> (MemoryStore, ObjectId) {
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
    .expect("construction succeeds");
    (store, constructed.root)
}

fn representation(store: &MemoryStore, root: ObjectId) -> FileContent {
    let canonical = store.canonical(root).expect("root bytes");
    classify(canonical).expect("classification")
}

fn apply(
    policy: ConstructionPolicy,
    store: &MemoryStore,
    root: ObjectId,
    edits: Vec<Edit>,
    replacements: &Replacements,
    final_len: u64,
) -> (MemoryStore, ObjectId, u64) {
    let stream = EditStream::new(store.logical_len_probe(root), edits).expect("valid stream");
    assert_eq!(stream.final_len(), final_len);
    let mut result = store.merged_clone();
    let constructed = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            store,
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
    (result, constructed.root, constructed.logical_len)
}

#[test]
fn exact_boundaries_at_every_accepted_cutoff() {
    for cutoff in CUTOFFS {
        let policy = policy_for(cutoff);
        for length in [0_u64, 1, cutoff - 1, cutoff, cutoff + 1] {
            let bytes = patterned(length as usize);
            let (store, root) = build(policy, &bytes);
            let content = representation(&store, root);
            assert_eq!(
                content.logical_len(),
                length,
                "cutoff {cutoff} length {length}"
            );
            match length {
                0 => assert!(matches!(content, FileContent::Chunked(_))),
                value if value < cutoff => {
                    assert!(matches!(content, FileContent::WholeFile { .. }))
                }
                _ => assert!(matches!(content, FileContent::Chunked(_))),
            }
            let read = read_back(&store, root).expect("reads back");
            assert_eq!(read, bytes, "cutoff {cutoff} length {length}");
        }
    }
}

#[test]
fn every_conversion_direction_reaches_the_fresh_construction_root() {
    for cutoff in CUTOFFS {
        let policy = policy_for(cutoff);
        let small_len = (cutoff / 2) as usize;
        let large_len = (cutoff + cutoff / 2) as usize;
        let base_small = patterned(small_len);
        let base_large = patterned(large_len);

        // Small -> small: an overwrite below the cutoff.
        {
            let (store, root) = build(policy, &base_small);
            let mut replacements = Replacements::new();
            replacements.push(noise(512));
            let mut expected = base_small.clone();
            expected[100..612].copy_from_slice(&noise(512));
            let (result, edited_root, len) = apply(
                policy,
                &store,
                root,
                vec![Edit::overwrite(100, 612)],
                &replacements,
                base_small.len() as u64,
            );
            assert_eq!(len, expected.len() as u64);
            assert_eq!(read_back(&result, edited_root).expect("read"), expected);
            let (_, fresh) = build(policy, &expected);
            assert_eq!(edited_root, fresh, "small -> small at {cutoff}");
        }

        // Small -> large: an insertion that crosses the cutoff.
        {
            let (store, root) = build(policy, &base_small);
            let addition = noise(cutoff as usize);
            let mut expected = base_small.clone();
            expected.splice(1_000..1_000, addition.iter().copied());
            let mut replacements = Replacements::new();
            let index = replacements.push(addition.clone());
            assert_eq!(index, 0);
            let (result, edited_root, len) = apply(
                policy,
                &store,
                root,
                vec![Edit::insert(1_000, addition.len() as u64)],
                &replacements,
                expected.len() as u64,
            );
            assert_eq!(len, expected.len() as u64);
            assert!(matches!(
                representation(&result, edited_root),
                FileContent::Chunked(_)
            ));
            assert_eq!(read_back(&result, edited_root).expect("read"), expected);
            let (_, fresh) = build(policy, &expected);
            assert_eq!(edited_root, fresh, "small -> large at {cutoff}");
        }

        // Large -> small: a deletion that drops below the cutoff. The deleted
        // range is never read.
        {
            let (store, root) = build(policy, &base_large);
            let keep = cutoff / 4;
            let mut expected = base_large[..keep as usize].to_vec();
            expected.extend_from_slice(&base_large[base_large.len() - keep as usize..]);
            let replacements = Replacements::new();
            let (result, edited_root, len) = apply(
                policy,
                &store,
                root,
                vec![Edit::delete(keep, base_large.len() as u64 - keep)],
                &replacements,
                expected.len() as u64,
            );
            assert_eq!(len, expected.len() as u64);
            assert!(matches!(
                representation(&result, edited_root),
                FileContent::WholeFile { .. }
            ));
            assert_eq!(read_back(&result, edited_root).expect("read"), expected);
            let (_, fresh) = build(policy, &expected);
            assert_eq!(edited_root, fresh, "large -> small at {cutoff}");
        }

        // Any -> empty.
        {
            let (store, root) = build(policy, &base_large);
            let replacements = Replacements::new();
            let (result, edited_root, len) = apply(
                policy,
                &store,
                root,
                vec![Edit::delete(0, base_large.len() as u64)],
                &replacements,
                0,
            );
            assert_eq!(len, 0);
            assert!(read_back(&result, edited_root).expect("read").is_empty());
            let (_, fresh) = build(policy, &[]);
            assert_eq!(edited_root, fresh, "any -> empty at {cutoff}");
        }
    }
}

#[test]
fn compressible_and_incompressible_inputs_agree_with_their_source() {
    let cutoff = 131_072_u64;
    let policy = policy_for(cutoff);
    for base in [support::repeat(200_000, 0x41), noise(200_000)] {
        let (store, root) = build(policy, &base);
        for replacement in [support::repeat(4_000, 0x41), noise(4_000)] {
            let mut expected = base.clone();
            expected[50_000..54_000].copy_from_slice(&replacement);
            let mut replacements = Replacements::new();
            replacements.push(replacement.clone());
            let (result, edited_root, _) = apply(
                policy,
                &store,
                root,
                vec![Edit::overwrite(50_000, 54_000)],
                &replacements,
                expected.len() as u64,
            );
            assert_eq!(read_back(&result, edited_root).expect("read"), expected);
        }
    }
}

#[test]
fn known_and_streamed_complete_lengths_agree() {
    let cutoff = 131_072_u64;
    let policy = policy_for(cutoff);
    for length in [cutoff - 1, cutoff, cutoff + 1] {
        let bytes = noise(length as usize);
        let (known_store, known_root) = build(policy, &bytes);
        let mut streamed_store = MemoryStore::new();
        let streamed = disabled_scope(|scope| {
            construct_stream(
                policy,
                &policy.capacities(),
                std::io::Cursor::new(bytes.clone()),
                &mut streamed_store,
                scope.child("content"),
            )
        })
        .expect("streamed construction");
        assert_eq!(
            known_root, streamed.root,
            "length {length}: streams agree with slices"
        );
        assert_eq!(read_back(&known_store, known_root).expect("read"), bytes);
    }
}

#[test]
fn unsupported_cutoffs_are_rejected_and_the_accepted_ones_are_not() {
    for rejected in [0_u64, 65_536, 98_304, 1_048_577, 2_097_152] {
        assert!(matches!(
            ConstructionPolicy::new(rejected, 8, 4).validated(),
            Err(ContentError::UnsupportedPolicy { .. })
        ));
    }
    for accepted in CUTOFFS {
        assert!(policy_for(accepted).validated().is_ok());
    }
}
