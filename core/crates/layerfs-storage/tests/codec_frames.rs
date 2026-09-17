//! The codec FFI boundary: what a hostile or malformed frame does there.
//!
//! Every product `unsafe` site lives in `encoding/codec.rs`, and until this
//! target existed no test called the codec directly: the raw FFI boundary was
//! exercised only indirectly, through saves and reads that always hand it bytes
//! this repository produced. These cases drive the *decode* entry points with a
//! valid frame, a declared-length lie, a truncated frame, a bad checksum and a
//! frame the profile's window does not cover, and require a typed refusal rather
//! than a panic, an overrun or a silent short read.

mod support;

use layerfs_storage::encoding::{CodecProfile, CompressionWorkspace, DecompressionWorkspace};
use layerfs_storage::{StorageCapacities, StorageError, StoragePolicy};

fn capacities(cutoff: u64) -> StorageCapacities {
    StorageCapacities::from_policy(StoragePolicy::new(1, cutoff, 8, 4)).expect("accepted policy")
}

fn encode(profile: CodecProfile, raw: &[u8]) -> Vec<u8> {
    let mut workspace = CompressionWorkspace::new().expect("encode workspace");
    workspace.compress(profile, raw).expect("a valid frame")
}

fn decode(profile: CodecProfile, frame: &[u8], raw_length: usize) -> Result<Vec<u8>, StorageError> {
    let mut workspace = DecompressionWorkspace::new().expect("decode workspace");
    workspace.decompress(profile, frame, raw_length)
}

#[test]
fn a_valid_frame_round_trips_through_both_decode_entry_points() {
    let capacities = capacities(131_072);
    let profile = CodecProfile::whole_file(&capacities);
    let raw = support::noise(96_000);
    let frame = encode(profile, &raw);
    assert_eq!(decode(profile, &frame, raw.len()).expect("round trip"), raw);

    // The prefix entry point borrows the base as a raw Zstandard prefix.
    let base = support::noise(96_000);
    let mut workspace = CompressionWorkspace::new().expect("encode workspace");
    let prefixed = workspace
        .compress_prefix(profile, &raw, &base)
        .expect("prefix frame");
    let mut decoder = DecompressionWorkspace::new().expect("decode workspace");
    assert_eq!(
        decoder
            .decompress_prefix(profile, &prefixed, raw.len(), &base)
            .expect("prefix round trip"),
        raw
    );
    // The reference is cleared after the call, so the same frame without its
    // prefix is not silently accepted against a stale borrow.
    let without = decoder.decompress(profile, &prefixed, raw.len());
    assert!(
        without.is_err(),
        "a prefix frame decoded without its prefix"
    );
}

#[test]
fn a_declared_length_that_contradicts_the_frame_is_refused() {
    let capacities = capacities(131_072);
    let profile = CodecProfile::whole_file(&capacities);
    let raw = support::noise(4_096);
    let frame = encode(profile, &raw);
    for claimed in [raw.len() - 1, raw.len() + 1, 0] {
        let error = decode(profile, &frame, claimed).expect_err("a lying declared length");
        assert!(
            matches!(
                error,
                StorageError::Integrity(_) | StorageError::CapacityExceeded { .. }
            ),
            "declared length {claimed} produced {error}"
        );
    }
}

#[test]
fn a_forged_declared_length_inside_the_header_is_refused_by_the_call_itself() {
    // The header is patched so the frame *declares* the same wrong length the
    // caller claims. The pre-call header check therefore passes and the refusal
    // has to come from the FFI call or the post-call length check: a destination
    // allocated from the caller's own validated length is never overrun.
    let capacities = capacities(131_072);
    let profile = CodecProfile::whole_file(&capacities);
    let raw = support::noise(200);
    let mut frame = encode(profile, &raw);
    assert_eq!(
        decode(profile, &frame, raw.len()).expect("the honest frame decodes"),
        raw
    );
    // A single-segment frame of 200 bytes carries its one-byte frame content size
    // immediately after the magic and the frame header descriptor. The patch is
    // checked to have moved that field: claiming the true length now fails, which
    // it could not do if the byte meant something else.
    frame[5] = 199;
    assert!(
        decode(profile, &frame, raw.len()).is_err(),
        "the declared content size did not move"
    );
    let error = decode(profile, &frame, 199).expect_err("the forged header is not believed");
    assert!(
        matches!(error, StorageError::Integrity(_)),
        "the forged frame produced {error}"
    );
}

#[test]
fn every_truncation_of_a_valid_frame_is_refused_without_panicking() {
    let capacities = capacities(131_072);
    let profile = CodecProfile::whole_file(&capacities);
    let raw = support::noise(2_048);
    let frame = encode(profile, &raw);
    for length in 0..frame.len() {
        let error = decode(profile, &frame[..length], raw.len())
            .expect_err("a truncated frame is never accepted");
        assert!(
            matches!(
                error,
                StorageError::Integrity(_) | StorageError::CapacityExceeded { .. }
            ),
            "truncation to {length} bytes produced {error}"
        );
    }
}

#[test]
fn a_frame_whose_checksum_no_longer_matches_is_refused() {
    let capacities = capacities(131_072);
    let profile = CodecProfile::whole_file(&capacities);
    let raw = support::noise(8_192);
    let mut frame = encode(profile, &raw);
    let last = frame.len() - 1;
    frame[last] ^= 0xff;
    let error = decode(profile, &frame, raw.len()).expect_err("a bad checksum is refused");
    assert!(
        matches!(error, StorageError::Integrity(_)),
        "the bad checksum produced {error}"
    );
}

#[test]
fn a_frame_beyond_the_profiles_window_is_refused() {
    // A frame produced under the 1 MiB cutoff's window log (20) offered to a
    // decoder whose profile is the 128 KiB cutoff's (18). The frame is short
    // because its content is highly compressible, so the profile's frame bound is
    // not what refuses it.
    let large = capacities(1_048_576);
    let small = capacities(131_072);
    let wide = CodecProfile::whole_file(&large);
    let narrow = CodecProfile::whole_file(&small);
    assert!(wide.window_log() > narrow.window_log());
    let raw = support::repeat(wide.raw_limit(), 0x5a);
    let frame = encode(wide, &raw);
    assert!(frame.len() <= narrow.frame_limit());

    // `raw_length` must be within the narrow profile's own raw bound for the
    // bounds check to be passed at all; the refusal below is therefore the
    // header's content-size and window decision, not a size bound.
    let refused = decode(narrow, &frame, 131_071).expect_err("the frame is not this profile's");
    assert!(
        matches!(refused, StorageError::Integrity(_)),
        "the wide frame produced {refused}"
    );

    // The window clause itself, recorded honestly: for a frame this profile's
    // grammar can address at all, the declared window never exceeds the content
    // size, and the content size is bounded by the profile's raw limit, which is
    // below the window. A frame that reached the window check would have to be
    // multi-segment, and every encoder in this repository emits single-segment
    // frames, so the clause is defence in depth with no reachable witness here.
    let largest = support::noise(narrow.raw_limit());
    let addressable = encode(narrow, &largest);
    assert_eq!(
        decode(narrow, &addressable, narrow.raw_limit()).expect("the largest addressable frame"),
        largest
    );
}
