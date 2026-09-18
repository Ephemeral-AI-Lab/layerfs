//! SHA-256 known-answer tests, because the O2 gate is blind to a wrong SHA-256.
//!
//! **The blind spot this file exists for.** `workload::oracle::read_back` compares
//! two digests that *the same* `Sha256` implementation produced: one over the bytes
//! the product returned, one over the bytes the recipe declared. If the
//! implementation is wrong in the same way on both sides, the two agree and the O2
//! gate reports a match. The gate detects **content mismatch**; it does not verify
//! the digest. Nothing else in the harness anchors `Sha256` to the standard, so a
//! transcription error in a round constant or a rotation would silently make every
//! published O2 gate a comparison of one wrong number with itself.
//!
//! These are the FIPS 180-4 vectors, with the two padding edges the standard calls
//! out explicitly: the 448-bit message (padding needs a second block, because
//! `1 + 448 + 64 > 512`) and the 896-bit message (the same edge one block later).
//! A padding bug that only shows up when the length lands in the last 64 bits of a
//! block is invisible to a one-block vector, so both are present.

use fs_bench_storage_content::workload::digest::{hex, sha256, Sha256};

/// `""`, `"abc"`, the 448-bit message and the 896-bit message.
const FIPS_VECTORS: [(&[u8], &str); 4] = [
    (
        b"",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    ),
    (
        b"abc",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    ),
    (
        b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    ),
    (
        b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
        "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
    ),
];

#[test]
fn fips_180_4_published_vectors_match() {
    for (bytes, expected) in FIPS_VECTORS {
        let digest = sha256(bytes);
        assert_eq!(
            hex(&digest),
            expected,
            "SHA-256 of the {}-byte vector diverged from FIPS 180-4",
            bytes.len()
        );
    }
}

#[test]
fn the_448_bit_padding_edge_is_a_second_block() {
    // 56 bytes: `1 + 448 + 64 = 513 > 512`, so the message cannot be padded inside
    // its own block. An implementation that writes the length into the first block
    // regardless still hashes one-block inputs correctly.
    let message = FIPS_VECTORS[2].0;
    assert_eq!(message.len(), 56, "the 448-bit vector must be 56 bytes");
    assert_eq!(
        message.len() % 64,
        56,
        "the length must land in the pad region"
    );
    assert_eq!(hex(&sha256(message)), FIPS_VECTORS[2].1);
}

#[test]
fn the_896_bit_padding_edge_is_a_second_block() {
    // 112 bytes = one full block plus the same 56-byte tail, so it exercises the
    // same edge with a non-empty buffer entering `finish`.
    let message = FIPS_VECTORS[3].0;
    assert_eq!(message.len(), 112, "the 896-bit vector must be 112 bytes");
    assert_eq!(
        message.len() % 64,
        48,
        "the tail must be 48 bytes before padding"
    );
    assert_eq!(hex(&sha256(message)), FIPS_VECTORS[3].1);
}

#[test]
fn the_million_a_vector_matches_across_many_blocks() {
    // The standard's long vector: 15,625 blocks, so a wrong block counter or a
    // length that is not wrapped correctly shows up here and nowhere else.
    let message = vec![b'a'; 1_000_000];
    assert_eq!(
        hex(&sha256(&message)),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn a_binary_multi_block_input_matches() {
    // Non-UTF-8 bytes, so a hasher that accidentally treated the input as text
    // (or normalised it) diverges.
    let message: Vec<u8> = (0..=255u8).cycle().take(768).collect();
    assert_eq!(
        hex(&sha256(&message)),
        "f3a25aa93aa2fbba28d79260535bbd6a5eb0fc1c24a8b0f04e12b484c1dfe363"
    );
}

#[test]
fn streaming_and_one_shot_agree_at_every_chunk_boundary() {
    // The harness streams a 500 MiB read-back through `HashingSink` in whatever
    // chunks the product returns, so an `update` that mishandles a partial buffer
    // would corrupt a measured O2 gate without failing any one-shot vector.
    let message: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    let one_shot = sha256(&message);
    for chunk in [1usize, 7, 31, 32, 63, 64, 65, 127, 128, 1000, 4096] {
        let mut hasher = Sha256::new();
        for slice in message.chunks(chunk) {
            hasher.update(slice);
        }
        assert_eq!(
            hasher.finish(),
            one_shot,
            "streaming in {chunk}-byte chunks diverged from the one-shot digest"
        );
    }
}

#[test]
fn an_empty_update_is_a_no_op() {
    let mut with_empty = Sha256::new();
    with_empty.update(b"");
    with_empty.update(b"abc");
    with_empty.update(b"");
    assert_eq!(with_empty.finish(), sha256(b"abc"));
}

#[test]
fn a_single_flipped_byte_changes_the_digest() {
    // The property the O2 gate actually relies on: different content must produce a
    // different digest. This is necessary but not sufficient, which is the whole
    // reason the vectors above exist.
    let base = vec![0x5au8; 4096];
    let mut flipped = base.clone();
    flipped[2048] ^= 0x01;
    assert_ne!(sha256(&base), sha256(&flipped));
}

#[test]
fn two_agreeing_wrong_digests_would_still_compare_equal() {
    // The blind spot, stated as a test rather than as a comment. The O2 gate is
    // `digest == expected_digest` where both sides came from this same function, so
    // equality is insensitive to the function being wrong. What makes the gate
    // meaningful is that the *expected* side is anchored to FIPS 180-4 above.
    use fs_bench_storage_content::workload::oracle::ReadBack;

    let agreed = ReadBack {
        bytes: 3,
        expected_bytes: 3,
        digest: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        expected_digest: "0000000000000000000000000000000000000000000000000000000000000000"
            .to_string(),
    };
    assert!(
        agreed.matches(),
        "identical digests on both sides must compare equal, however wrong they are"
    );
}
