//! The oracle is independent of the product, and these tests hold it to that.
//!
//! `Expectation::spliced` is the oracle for every edit family. It computes the
//! expected logical bytes **from the recipe** by streaming `base[..start]`,
//! `replacement` and `base[end..]` through a hasher, without ever materialising the
//! spliced result. That streaming shortcut is what makes verifying a 500 MiB edit
//! cost the oracle nothing, and it is also the one place a splice could silently
//! differ from a materialised one — an off-by-one in the boundary arithmetic would
//! produce a digest that agrees with nothing and would be blamed on the product.
//!
//! So every edge is checked against a naive materialised splice: head, middle,
//! tail, a pure insert (`start == end`), a pure delete (empty replacement) and the
//! zero-length extremes.

use fs_bench_storage_content::workload::digest::{hex, sha256};
use fs_bench_storage_content::workload::oracle::{
    sample_ranges, Census, CensusEntry, Expectation, HashingSink, ReadBack, TreeSampleBounds,
};

use std::io::Write;

/// Compares the streaming splice oracle with a naive materialised splice.
fn assert_splice_matches_naive(base: &[u8], start: u64, end: u64, replacement: &[u8]) {
    let mut naive = Vec::new();
    naive.extend_from_slice(&base[..start as usize]);
    naive.extend_from_slice(replacement);
    naive.extend_from_slice(&base[end as usize..]);

    let streamed = Expectation::spliced(base, start, end, replacement);
    assert_eq!(
        streamed.logical_len,
        naive.len() as u64,
        "spliced({start}..{end}) length disagreed with the materialised splice"
    );
    assert_eq!(
        streamed.sha256,
        sha256(&naive),
        "spliced({start}..{end}) digest disagreed with the materialised splice"
    );
    assert_eq!(streamed.digest_hex(), hex(&sha256(&naive)));
}

#[test]
fn a_head_splice_matches_a_materialised_splice() {
    let base = b"0123456789";
    assert_splice_matches_naive(base, 0, 3, b"abc");
    assert_splice_matches_naive(base, 0, 1, b"Z");
}

#[test]
fn a_middle_splice_matches_a_materialised_splice() {
    let base = b"0123456789";
    assert_splice_matches_naive(base, 3, 6, b"XYZ");
    assert_splice_matches_naive(base, 4, 6, b"LONGER-THAN-REMOVED");
}

#[test]
fn a_tail_splice_matches_a_materialised_splice() {
    let base = b"0123456789";
    assert_splice_matches_naive(base, 7, 10, b"TT");
    assert_splice_matches_naive(base, 9, 10, b"");
}

#[test]
fn a_pure_insert_matches_a_materialised_splice() {
    // `start == end`: nothing is removed, so the logical length must grow by the
    // whole replacement. An implementation that used `end - start + 1` would be
    // wrong here and right everywhere else.
    let base = b"0123456789";
    assert_splice_matches_naive(base, 0, 0, b"HEAD");
    assert_splice_matches_naive(base, 5, 5, b"++");
    assert_splice_matches_naive(base, 10, 10, b"TAIL");
}

#[test]
fn a_pure_delete_matches_a_materialised_splice() {
    let base = b"0123456789";
    assert_splice_matches_naive(base, 2, 5, b"");
    assert_splice_matches_naive(base, 0, 10, b"");
    assert_splice_matches_naive(base, 0, 0, b"");
}

#[test]
fn the_zero_length_edges_match_a_materialised_splice() {
    // An empty base with an empty splice is the identity; an empty base with a
    // replacement is a pure construction. Both are reachable from a truncate-to-zero
    // base followed by an edit.
    assert_splice_matches_naive(b"", 0, 0, b"");
    assert_splice_matches_naive(b"", 0, 0, b"new");
    assert_splice_matches_naive(b"a", 0, 0, b"");
    assert_splice_matches_naive(b"a", 0, 1, b"");
}

#[test]
fn the_splice_oracle_recomputes_rather_than_reusing_the_base() {
    // If `spliced` returned the base's own expectation, a failed edit would be
    // reported as a match. Every non-identity splice must differ from the base.
    let base = b"0123456789";
    let unchanged = Expectation::of(base);
    let changed = Expectation::spliced(base, 4, 5, b"Q");
    assert_ne!(changed.sha256, unchanged.sha256);
    assert_eq!(
        changed.logical_len, unchanged.logical_len,
        "an overwrite keeps the length"
    );

    let identity = Expectation::spliced(base, 0, 0, b"");
    assert_eq!(
        identity.sha256, unchanged.sha256,
        "the empty splice is the identity"
    );
}

#[test]
fn an_expectation_of_bytes_hashes_those_bytes() {
    let bytes = b"the fixture recipe, not the artifact";
    let expectation = Expectation::of(bytes);
    assert_eq!(expectation.logical_len, bytes.len() as u64);
    assert_eq!(expectation.sha256, sha256(bytes));
    assert_eq!(expectation.digest_hex().len(), 64);
}

#[test]
fn a_read_back_whose_bytes_differ_fails_the_digest_gate() {
    // The O2 comparison itself. Same length, different content: the length check
    // passes and the digest check must not.
    let recipe = Expectation::of(b"expected logical bytes");
    let mut sink = HashingSink::new();
    sink.write_all(b"observed logical bytes")
        .expect("hashing never fails");
    let observed = ReadBack {
        bytes: sink.bytes(),
        expected_bytes: recipe.logical_len,
        digest: hex(&sink.finish()),
        expected_digest: recipe.digest_hex(),
    };
    assert!(
        observed.length_matches(),
        "the two strings are the same length"
    );
    assert!(
        !observed.digest_matches(),
        "different bytes must not produce the same digest"
    );
    assert!(
        !observed.matches(),
        "the O2 gate must fail on a content mismatch"
    );
}

#[test]
fn a_read_back_with_the_wrong_length_fails_even_when_the_prefix_agrees() {
    let recipe = Expectation::of(b"abc");
    let mut sink = HashingSink::new();
    sink.write_all(b"abcd").expect("hashing never fails");
    let observed = ReadBack {
        bytes: sink.bytes(),
        expected_bytes: recipe.logical_len,
        digest: hex(&sink.finish()),
        expected_digest: recipe.digest_hex(),
    };
    assert!(!observed.length_matches());
    assert!(!observed.matches());
}

#[test]
fn a_read_back_that_matches_passes_both_halves() {
    let recipe = Expectation::of(b"the logical bytes");
    let mut sink = HashingSink::new();
    sink.write_all(b"the logical bytes")
        .expect("hashing never fails");
    let observed = ReadBack {
        bytes: sink.bytes(),
        expected_bytes: recipe.logical_len,
        digest: hex(&sink.finish()),
        expected_digest: recipe.digest_hex(),
    };
    assert!(observed.length_matches());
    assert!(observed.digest_matches());
    assert!(observed.matches());
}

#[test]
fn the_hashing_sink_streams_to_the_same_digest_as_one_shot() {
    // The sink is what a read-back is streamed through, so a chunk-boundary bug in
    // it would corrupt every O2 gate while the one-shot digest stayed correct.
    let payload: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    for chunk in [1usize, 13, 64, 65, 999, 5000] {
        let mut sink = HashingSink::new();
        for slice in payload.chunks(chunk) {
            let written = sink.write(slice).expect("hashing never fails");
            assert_eq!(
                written,
                slice.len(),
                "the sink must absorb every byte offered"
            );
        }
        assert_eq!(sink.bytes(), payload.len() as u64);
        assert_eq!(
            sink.finish(),
            sha256(&payload),
            "chunked in {chunk}-byte pieces"
        );
    }
}

#[test]
fn the_hashing_sink_retains_nothing() {
    // An oracle that allocated the answer would change the memory figure it exists
    // to check, so the sink counts and hashes and holds no payload.
    let mut sink = HashingSink::new();
    assert_eq!(sink.bytes(), 0);
    sink.write_all(&vec![7u8; 1 << 20])
        .expect("hashing never fails");
    assert_eq!(sink.bytes(), 1 << 20);
    // `HashingSink` has no accessor for the bytes it absorbed, by construction.
    let digest = sink.finish();
    assert_eq!(digest, sha256(&vec![7u8; 1 << 20]));
}

#[test]
fn sampled_ranges_stay_inside_the_file_and_the_frozen_bounds() {
    let bounds = TreeSampleBounds::default();
    assert_eq!(bounds.maximum_files, 11);
    assert_eq!(bounds.maximum_directories, 11);
    assert_eq!(bounds.ranges_per_file, 3);
    assert_eq!(bounds.range_bytes, 65_536);

    assert!(
        sample_ranges(0, bounds).is_empty(),
        "a zero-length file has no ranges"
    );

    let tiny = sample_ranges(1, bounds);
    assert_eq!(
        tiny,
        vec![(0, 1)],
        "a one-byte file yields one one-byte range"
    );

    let short = sample_ranges(100, bounds);
    assert_eq!(
        short,
        vec![(0, 100)],
        "a file shorter than the window is read whole"
    );

    let long = sample_ranges(200_000, bounds);
    assert_eq!(
        long.len(),
        3,
        "a long file is sampled at head, middle and tail"
    );
    for (start, end) in &long {
        assert!(end > start, "an empty range must be filtered out");
        assert!(
            *end <= 200_000,
            "a range must not run past the end of the file"
        );
        assert!(end - start <= bounds.range_bytes);
    }
    let mut sorted = long.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, long, "ranges are returned in ascending order");
}

#[test]
fn sampled_ranges_never_exceed_the_declared_range_count() {
    let bounds = TreeSampleBounds {
        ranges_per_file: 1,
        ..TreeSampleBounds::default()
    };
    assert_eq!(sample_ranges(200_000, bounds).len(), 1);
    let two = TreeSampleBounds {
        ranges_per_file: 2,
        ..TreeSampleBounds::default()
    };
    assert_eq!(sample_ranges(200_000, two).len(), 2);
}

#[test]
fn the_census_is_the_recipe_and_totals_its_own_entries() {
    let census = Census {
        entries: vec![
            CensusEntry {
                key: "a/one".to_string(),
                len: 10,
            },
            CensusEntry {
                key: "a/two".to_string(),
                len: 32,
            },
        ],
    };
    assert_eq!(census.len(), 2);
    assert!(!census.is_empty());
    assert_eq!(census.total_bytes(), 42);
    assert_eq!(census.entry("a/two").map(|entry| entry.len), Some(32));
    assert_eq!(census.entry("absent"), None);
    assert!(Census::default().is_empty());
    assert_eq!(Census::default().total_bytes(), 0);
}
