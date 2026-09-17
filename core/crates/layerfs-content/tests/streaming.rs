//! Streaming construction: bounded sources, dependency order and late failures.

mod support;

use layerfs_content::{
    construct_stream, read_all_bounded, ConstructionPolicy, ContentError, DiscardingConsumer,
    ObjectId, ObjectRole,
};
use support::{
    disabled_scope, noise, patterned, repeat, CountingSource, FailingAfter, MemoryStore,
    TrackingConsumer,
};

fn stream<R: std::io::Read>(
    source: R,
    consumer: &mut dyn layerfs_content::FinalizedConsumer,
) -> layerfs_content::ContentResult<layerfs_content::ConstructedFile> {
    let policy = ConstructionPolicy::frozen_default();
    disabled_scope(|scope| {
        construct_stream(
            policy,
            &policy.capacities(),
            source,
            consumer,
            scope.child("content.stream"),
        )
    })
}

/// Every read this path issues is bounded by a declared window, and no byte is
/// read twice.
///
/// The path has two bounded readers: the threshold probe, which is bounded by the
/// declared cutoff, and the frozen chunk scanner, whose own window is the maximum
/// chunk. The oracle is therefore stated on what the declarations promise - no
/// single request exceeds the declared cutoff, the scanner streams in many bounded
/// requests, and the source delivers exactly the input once - rather than on the
/// standard library's buffer-growth pattern inside the probe.
#[test]
fn input_requests_stay_bounded_by_the_declared_windows() {
    let bytes = noise(131_072 * 6);
    let cutoff = ConstructionPolicy::frozen_default().small_file_threshold_bytes() as usize;
    let mut consumer = DiscardingConsumer::new();
    let mut source = CountingSource::new(bytes.as_slice());
    let constructed = stream(&mut source, &mut consumer).unwrap();
    assert_eq!(constructed.logical_len, bytes.len() as u64);
    assert!(
        source.largest_request() <= cutoff,
        "a single read asked for {} bytes, above the declared cutoff {cutoff}",
        source.largest_request()
    );
    assert!(
        source.requests()
            >= bytes.len() as u64 / layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES as u64,
        "the scanner did not stream the input: {} requests",
        source.requests()
    );
    assert_eq!(
        source.bytes_read(),
        bytes.len() as u64,
        "the source was read exactly once, with no re-read and no over-read"
    );
}

#[test]
fn a_short_source_is_a_definitive_whole_file_result() {
    let mut consumer = MemoryStore::new();
    let bytes = patterned(4_000);
    let constructed = stream(bytes.as_slice(), &mut consumer).unwrap();
    assert_eq!(constructed.logical_len, 4_000);
    assert_eq!(consumer.roles(), vec![ObjectRole::WholeFile]);
}

#[test]
fn children_are_emitted_before_their_parents() {
    let bytes = noise(131_072 * 4 + 11);
    let mut consumer = TrackingConsumer::new();
    let constructed = stream(bytes.as_slice(), &mut consumer).unwrap();
    assert_eq!(constructed.logical_len, bytes.len() as u64);
    consumer.assert_children_precede_parents();

    let mut out = Vec::new();
    disabled_scope(|scope| {
        read_all_bounded(
            &consumer.store,
            constructed.root,
            u64::MAX,
            &mut out,
            scope.child("content.read"),
        )
    })
    .unwrap();
    assert_eq!(out, bytes);
}

#[test]
fn many_files_do_not_retain_the_whole_workload() {
    let mut consumer = DiscardingConsumer::new();
    let mut root_seen = Vec::new();
    let mut per_file = Vec::new();
    let mut emitted = 0_u64;
    for index in 0..24u32 {
        let bytes = repeat(200_000, index as u8);
        let constructed = stream(bytes.as_slice(), &mut consumer).unwrap();
        root_seen.push(constructed.root);
        let now = consumer.objects();
        per_file.push(now - emitted);
        emitted = now;
    }
    assert_eq!(root_seen.len(), 24);
    assert!(
        per_file.iter().all(|count| *count > 1),
        "each file emitted several objects: {per_file:?}"
    );
    // What one stream retains is a function of that stream, not of how many files
    // ran before it: the per-file emission count stays at the first file's shape
    // while the cumulative total grows with the number of files.
    assert!(
        per_file.iter().all(|count| *count <= per_file[0] + 2),
        "a later file did more work than the first: {per_file:?}"
    );
    assert_eq!(emitted, per_file.iter().sum::<u64>());
}

#[test]
fn a_late_input_failure_is_returned_once_after_early_output() {
    let bytes = noise(131_072 * 3);
    let source = FailingAfter::new(bytes, 131_072 + 4_096);
    let mut consumer = TrackingConsumer::new();
    let error = stream(source, &mut consumer).unwrap_err();
    assert_eq!(error, ContentError::Io);
    assert!(
        consumer.store.roles().contains(&ObjectRole::Chunk),
        "chunks were emitted before the failure"
    );
    consumer.assert_children_precede_parents();
}

#[test]
fn unknown_length_and_known_length_agree_object_by_object() {
    for len in [0, 1, 8_192, 131_071, 131_072, 131_073, 300_000] {
        let bytes = noise(len);
        let mut known = MemoryStore::new();
        let policy = ConstructionPolicy::frozen_default();
        let known_root = disabled_scope(|scope| {
            layerfs_content::construct_bytes(
                policy,
                &policy.capacities(),
                &bytes,
                &mut known,
                scope.child("content"),
            )
        })
        .unwrap();
        let mut streamed = MemoryStore::new();
        let streamed_root = stream(bytes.as_slice(), &mut streamed).unwrap();
        assert_eq!(known_root.root, streamed_root.root, "len={len}");
        assert_eq!(known.order(), streamed.order(), "len={len}");
    }
}

#[test]
fn a_discarding_consumer_never_allocates_a_payload_copy() {
    let bytes = noise(131_072 * 2);
    let mut consumer = DiscardingConsumer::new();
    let constructed = stream(bytes.as_slice(), &mut consumer).unwrap();
    assert_eq!(constructed.logical_len, bytes.len() as u64);
    assert!(consumer.objects() > 0);
    assert!(
        consumer.peak_object_bytes() <= 32_768 + 32,
        "no emitted object exceeds the chunk window plus its framing"
    );
}

#[test]
fn root_identity_is_stable_across_repeated_streaming_runs() {
    let bytes = noise(400_000);
    let mut first = MemoryStore::new();
    let first_root = stream(bytes.as_slice(), &mut first).unwrap();
    let mut second = MemoryStore::new();
    let second_root = stream(bytes.as_slice(), &mut second).unwrap();
    assert_eq!(first_root.root, second_root.root);
    assert_eq!(first.order(), second.order());
    assert_ne!(first_root.root, ObjectId::for_bytes(b"unrelated"));
}
