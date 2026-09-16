//! Complete-file construction: representation choice, frozen partitions, readback.

mod support;

use layerfs_content::file::cdc::{FastCdc, MAXIMUM_CHUNK_BYTES, MINIMUM_CHUNK_BYTES};
use layerfs_content::file::mapping::{decode_file_state, decode_node, ExtentNode};
use layerfs_content::{
    construct_bytes, construct_stream, read_all, ConstructionPolicy, ContentError,
    DiscardingConsumer, FileContent, ObjectRole, Representation,
};
use support::{
    disabled_scope, noise, patterned, read_back, repeat, CountingSource, FailingAfter, MemoryStore,
};

const CUTOFF: usize = 131_072;

fn policy() -> ConstructionPolicy {
    ConstructionPolicy::frozen_default()
}

fn build(bytes: &[u8]) -> (MemoryStore, layerfs_content::ConstructedFile) {
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
    .expect("construction succeeds");
    (store, constructed)
}

#[test]
fn representation_boundaries_are_exact_and_exclusive() {
    assert_eq!(policy().representation(0), Representation::Empty);
    assert_eq!(policy().representation(1), Representation::WholeFile);
    assert_eq!(
        policy().representation(CUTOFF as u64 - 1),
        Representation::WholeFile
    );
    assert_eq!(
        policy().representation(CUTOFF as u64),
        Representation::Chunked
    );
    assert_eq!(
        policy().representation(CUTOFF as u64 + 1),
        Representation::Chunked
    );
}

#[test]
fn empty_file_uses_the_defined_file_state_form() {
    let (store, constructed) = build(&[]);
    assert_eq!(constructed.logical_len, 0);
    let canonical = store.canonical(constructed.root).expect("root stored");
    let state = decode_file_state(canonical).expect("file state");
    assert_eq!(state.logical_len, 0);
    assert_eq!(state.extent_count, 0);
    let leaf = store
        .canonical(state.mapping_root)
        .expect("empty mapping page stored");
    assert_eq!(
        decode_node(leaf).expect("empty leaf"),
        ExtentNode::Leaf {
            subtree_logical_bytes: 0,
            extents: Vec::new(),
        }
    );
    assert_eq!(
        store.roles(),
        vec![ObjectRole::ExtentLeaf, ObjectRole::FileState]
    );
    assert_eq!(
        read_back(&store, constructed.root).unwrap(),
        Vec::<u8>::new()
    );
}

#[test]
fn one_byte_file_is_a_whole_file_object() {
    let (store, constructed) = build(&[0xa5]);
    assert_eq!(constructed.logical_len, 1);
    assert_eq!(store.roles(), vec![ObjectRole::WholeFile]);
    assert_eq!(read_back(&store, constructed.root).unwrap(), vec![0xa5]);
}

#[test]
fn below_cutoff_uses_one_whole_file_object_and_reads_back_exactly() {
    for len in [
        1,
        2,
        MINIMUM_CHUNK_BYTES - 1,
        MINIMUM_CHUNK_BYTES,
        CUTOFF - 1,
    ] {
        let bytes = patterned(len);
        let (store, constructed) = build(&bytes);
        assert_eq!(constructed.logical_len, len as u64, "len={len}");
        assert_eq!(store.roles(), vec![ObjectRole::WholeFile], "len={len}");
        assert_eq!(store.len(), 1, "len={len}");
        assert_eq!(
            read_back(&store, constructed.root).unwrap(),
            bytes,
            "len={len}"
        );
    }
}

#[test]
fn at_and_above_cutoff_uses_a_chunked_tree() {
    for len in [CUTOFF, CUTOFF + 1, CUTOFF * 3 + 7] {
        let bytes = noise(len);
        let (store, constructed) = build(&bytes);
        assert_eq!(constructed.logical_len, len as u64, "len={len}");
        assert!(store.roles().contains(&ObjectRole::FileState), "len={len}");
        assert!(store.roles().contains(&ObjectRole::Chunk), "len={len}");
        assert!(!store.roles().contains(&ObjectRole::WholeFile), "len={len}");
        assert_eq!(
            read_back(&store, constructed.root).unwrap(),
            bytes,
            "len={len}"
        );
    }
}

#[test]
fn chunks_respect_the_frozen_size_window() {
    let bytes = noise(CUTOFF * 4);
    let (store, _) = build(&bytes);
    let mut lengths = Vec::new();
    for (id, role) in store.order() {
        if *role != ObjectRole::Chunk {
            continue;
        }
        let canonical = store.canonical(*id).unwrap();
        lengths.push(canonical.len() - 9 - 4 - 8);
    }
    assert!(lengths.len() >= 16, "a 512 KiB input produces many chunks");
    let (last, rest) = lengths.split_last().expect("at least one chunk");
    assert!(rest.iter().all(|len| *len >= MINIMUM_CHUNK_BYTES));
    assert!(rest.iter().all(|len| *len <= MAXIMUM_CHUNK_BYTES));
    assert!(*last > 0 && *last <= MAXIMUM_CHUNK_BYTES);
}

#[test]
fn chunk_partition_is_frozen_for_the_reference_input() {
    let bytes = noise(100_000);
    let mut lengths = Vec::new();
    let counters = FastCdc::new()
        .scan(bytes.as_slice(), |chunk| {
            lengths.push(chunk.len());
            Ok(())
        })
        .unwrap();
    assert_eq!(lengths, [16_396, 17_093, 16_413, 20_273, 19_016, 10_809]);
    assert_eq!(counters.bytes_scanned, 100_000);
    assert_eq!(counters.chunks_emitted, 6);
}

#[test]
fn known_length_and_streaming_inputs_agree_exactly() {
    for len in [0, 1, 4_096, CUTOFF - 1, CUTOFF, CUTOFF + 11, CUTOFF * 2] {
        let bytes = patterned(len);
        let mut known_store = MemoryStore::new();
        let known = disabled_scope(|scope| {
            construct_bytes(
                policy(),
                &policy().capacities(),
                &bytes,
                &mut known_store,
                scope.child("content"),
            )
        })
        .unwrap();

        let mut stream_store = MemoryStore::new();
        let streamed = disabled_scope(|scope| {
            construct_stream(
                policy(),
                &policy().capacities(),
                bytes.as_slice(),
                &mut stream_store,
                scope.child("content"),
            )
        })
        .unwrap();

        assert_eq!(known.root, streamed.root, "len={len}");
        assert_eq!(known.logical_len, streamed.logical_len, "len={len}");
        assert_eq!(
            known_store.canonical_bytes(),
            stream_store.canonical_bytes(),
            "len={len}"
        );
        assert_eq!(known_store.roles(), stream_store.roles(), "len={len}");
    }
}

#[test]
fn construction_is_deterministic_for_the_same_input() {
    let bytes = noise(CUTOFF * 2 + 5);
    let (first, first_root) = build(&bytes);
    let (second, second_root) = build(&bytes);
    assert_eq!(first_root.root, second_root.root);
    assert_eq!(first.order(), second.order());
}

#[test]
fn compressible_and_incompressible_inputs_agree_with_their_source() {
    for bytes in [repeat(CUTOFF * 2, 0x5a), noise(CUTOFF * 2)] {
        let (store, constructed) = build(&bytes);
        assert_eq!(read_back(&store, constructed.root).unwrap(), bytes);
    }
}

#[test]
fn streamed_source_requests_stay_bounded() {
    let bytes = noise(CUTOFF * 2);
    let mut consumer = DiscardingConsumer::new();
    let mut source = CountingSource::new(bytes.as_slice());
    let constructed = disabled_scope(|scope| {
        construct_stream(
            policy(),
            &policy().capacities(),
            &mut source,
            &mut consumer,
            scope.child("content"),
        )
    })
    .unwrap();
    assert_eq!(constructed.logical_len, bytes.len() as u64);
    assert!(
        consumer.objects() > 0,
        "construction emitted finalized objects"
    );
    assert!(consumer.canonical_bytes() >= constructed.logical_len);
    assert!(consumer.peak_object_bytes() <= MAXIMUM_CHUNK_BYTES as u64 + 32);
    assert!(
        source.largest_request() <= CUTOFF,
        "the threshold probe and the scanner both stay inside the declared window"
    );
    assert!(
        source.requests() > 1,
        "the scanner reads the source more than once"
    );
}

#[test]
fn late_input_failure_after_early_output_is_returned_once() {
    let bytes = noise(CUTOFF * 2);
    let mut store = MemoryStore::new();
    let source = FailingAfter::new(bytes, CUTOFF + 4_096);
    let error = disabled_scope(|scope| {
        construct_stream(
            policy(),
            &policy().capacities(),
            source,
            &mut store,
            scope.child("content"),
        )
    })
    .unwrap_err();
    assert_eq!(error, ContentError::Io);
    assert!(
        store.roles().contains(&ObjectRole::Chunk),
        "early chunks were emitted before the failure"
    );
}

#[test]
fn unsupported_policy_values_are_rejected_before_work() {
    let rejected = ConstructionPolicy::new(1_048_576, 8, 4);
    assert_eq!(
        rejected.validated(),
        Err(ContentError::UnsupportedPolicy {
            field: "small_file_threshold_bytes"
        })
    );
    assert_eq!(
        ConstructionPolicy::new(CUTOFF as u64, 7, 4).validated(),
        Err(ContentError::UnsupportedPolicy {
            field: "whole_file_delta_max_depth"
        })
    );
    assert_eq!(
        ConstructionPolicy::new(CUTOFF as u64, 8, 5).validated(),
        Err(ContentError::UnsupportedPolicy {
            field: "chunk_delta_max_depth"
        })
    );
    assert!(policy().validated().is_ok());
}

#[test]
fn whole_file_payload_is_classified_without_a_db() {
    let bytes = patterned(1_000);
    let (store, constructed) = build(&bytes);
    let mut out = Vec::new();
    disabled_scope(|scope| {
        read_all(
            &store,
            constructed.root,
            &mut out,
            scope.child("content.read"),
        )
    })
    .unwrap();
    assert_eq!(out, bytes);
    let classified =
        layerfs_content::file::classify(store.canonical(constructed.root).unwrap()).unwrap();
    assert_eq!(
        classified,
        FileContent::WholeFile {
            logical_len: bytes.len() as u64
        }
    );
}
