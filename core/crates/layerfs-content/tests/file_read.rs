//! Logical file and range reads, including repeated demands of one payload.

mod support;

use layerfs_content::file::mapping::decode_file_state;
use layerfs_content::{read_all, read_range, ContentError, ObjectRole};
use support::{
    build_file, disabled_scope, noise, patterned, read_back, repeat, CountingStore, MemoryStore,
    PoisonedStore,
};

fn chunked(len: usize) -> (MemoryStore, layerfs_content::ConstructedFile, Vec<u8>) {
    let bytes = noise(len);
    let (store, constructed) = build_file(&bytes);
    (store, constructed, bytes)
}

#[test]
fn full_read_returns_exact_bytes_for_every_representation() {
    for bytes in [Vec::new(), vec![7], patterned(9_000), noise(131_072 + 5)] {
        let (store, constructed) = build_file(&bytes);
        assert_eq!(
            read_back(&store, constructed.root).unwrap(),
            bytes,
            "length {}",
            bytes.len()
        );
    }
}

#[test]
fn ranges_cross_extents_and_page_boundaries() {
    let (store, constructed, bytes) = chunked(131_072 * 5 + 3);
    let ranges = [
        0..1,
        0..bytes.len() as u64,
        1..2,
        8_191..8_193,
        16_000..70_000,
        131_071..131_073,
        (bytes.len() as u64 - 3)..(bytes.len() as u64),
        (bytes.len() as u64 - 1)..(bytes.len() as u64),
    ];
    for range in ranges {
        let start = range.start.min(range.end) as usize;
        let end = range.end.max(range.start) as usize;
        let mut out = Vec::new();
        disabled_scope(|scope| {
            read_range(
                &store,
                constructed.root,
                range.clone(),
                &mut out,
                scope.child("content.read"),
            )
        })
        .unwrap_or_else(|error| panic!("range {range:?} failed: {error}"));
        assert_eq!(out, bytes[start..end], "range {range:?}");
    }
}

#[test]
fn empty_range_at_end_of_file_is_empty_and_at_eof_bound_is_allowed() {
    let (store, constructed, _) = chunked(200_000);
    let logical = constructed.logical_len;
    let mut out = Vec::new();
    disabled_scope(|scope| {
        read_range(
            &store,
            constructed.root,
            logical..logical,
            &mut out,
            scope.child("content.read"),
        )
    })
    .unwrap();
    assert!(out.is_empty());
}

#[test]
fn out_of_bounds_and_inverted_ranges_fail() {
    let (store, constructed, _) = chunked(150_000);
    let logical = constructed.logical_len;
    let inverted = std::ops::Range { start: 10, end: 5 };
    for range in [logical + 1..logical + 2, 0..logical + 1, inverted] {
        let mut out = Vec::new();
        let error = disabled_scope(|scope| {
            read_range(
                &store,
                constructed.root,
                range.clone(),
                &mut out,
                scope.child("content.read"),
            )
        })
        .unwrap_err();
        assert!(
            matches!(error, ContentError::InvalidRange { .. }),
            "range {range:?} produced {error}"
        );
    }
}

#[test]
fn whole_file_root_rejects_a_range_beyond_its_payload() {
    let bytes = patterned(1_000);
    let (store, constructed) = build_file(&bytes);
    let mut out = Vec::new();
    let error = disabled_scope(|scope| {
        read_range(
            &store,
            constructed.root,
            0..1_001,
            &mut out,
            scope.child("content.read"),
        )
    })
    .unwrap_err();
    assert!(matches!(
        error,
        ContentError::InvalidRange { length: 1000, .. }
    ));
}

#[test]
fn repeated_demands_of_one_payload_read_it_once_and_emit_it_every_time() {
    // A repetitive input makes the frozen chunker emit identical chunks, so the
    // extent tree references the same payload object many times.
    let block = repeat(8_192, 0x33);
    let mut bytes = Vec::new();
    for _ in 0..24 {
        bytes.extend_from_slice(&block);
    }
    let (store, constructed) = build_file(&bytes);
    let state = decode_file_state(store.canonical(constructed.root).unwrap()).unwrap();
    assert!(state.extent_count > 4, "the tree references many extents");

    let mut out = Vec::new();
    let counters = disabled_scope(|scope| {
        read_all(
            &store,
            constructed.root,
            &mut out,
            scope.child("content.read"),
        )
    })
    .unwrap();
    assert_eq!(out, bytes);
    assert_eq!(counters.payload_bytes_read, bytes.len() as u64);
    assert!(
        counters.payload_ids_read < state.extent_count,
        "distinct payloads {} are fewer than extents {}",
        counters.payload_ids_read,
        state.extent_count
    );
    assert!(
        counters.max_payload_batch as usize <= layerfs_content::file::mapping::READ_WAVE_OBJECTS
    );
}

#[test]
fn a_wave_is_released_before_the_next_one_is_read() {
    let bytes = noise(131_072 * 8);
    let (store, constructed) = build_file(&bytes);
    let tracker = CountingStore::new(&store);
    let mut out = Vec::new();
    let counters = disabled_scope(|scope| {
        read_all(
            &tracker,
            constructed.root,
            &mut out,
            scope.child("content.read"),
        )
    })
    .unwrap();
    assert_eq!(out, bytes);
    assert!(
        counters.payload_batches_read > 1,
        "the read used several waves"
    );
    assert!(tracker.peak_live_payloads() <= layerfs_content::file::mapping::READ_WAVE_OBJECTS);
}

#[test]
fn a_provider_failure_is_returned_once_and_stops_the_read() {
    let (store, constructed, _) = chunked(200_000);
    let chunk = store
        .order()
        .iter()
        .find(|(_, role)| *role == ObjectRole::Chunk)
        .map(|(id, _)| *id)
        .expect("a chunk exists");
    let poisoned = PoisonedStore::new(&store, chunk);
    let mut out = Vec::new();
    let error = disabled_scope(|scope| {
        read_all(
            &poisoned,
            constructed.root,
            &mut out,
            scope.child("content.read"),
        )
    })
    .unwrap_err();
    assert_eq!(error, ContentError::MissingObject);
    assert!(out.len() < 200_000, "the read stopped at the failure");
}

/// The read wave never exceeds its declared object and byte window.
///
/// A 24 MiB chunked file carries about 1 500 payloads; the reader must acquire
/// them in bounded batches rather than in one demand set, and the largest batch it
/// ever asked for must be inside the declared window.
#[test]
fn a_large_read_acquires_payloads_in_bounded_batches() {
    use layerfs_content::file::mapping::{READ_WAVE_BYTES, READ_WAVE_OBJECTS};
    let bytes = noise(24 * 1024 * 1024 + 1);
    let (store, constructed, _) = chunked(bytes.len());
    let mut out = Vec::new();
    let counters = disabled_scope(|scope| {
        layerfs_content::read_all_bounded(
            &store,
            constructed.root,
            u64::MAX,
            &mut out,
            scope.child("content.read"),
        )
    })
    .expect("large read");
    assert_eq!(out.len(), bytes.len());
    assert_eq!(out, bytes);
    assert!(
        counters.payload_batches_read > 1,
        "the fixture must need several batches: {counters:?}"
    );
    // The object window is the measured one: a 24 MiB read fills the wave to its
    // declared bound rather than staying small. The byte window is *derived* from
    // that same bound at the largest chunk record, so it is asserted as the
    // identity it is and not presented as a second, independent measurement.
    assert_eq!(
        counters.max_payload_batch, READ_WAVE_OBJECTS as u64,
        "the largest batch is exactly the declared object window: {counters:?}"
    );
    assert_eq!(
        READ_WAVE_BYTES,
        READ_WAVE_OBJECTS * 32_768,
        "the declared byte window is the object bound at the largest chunk"
    );
    assert!(
        counters.payload_bytes_read >= bytes.len() as u64,
        "the read charged the payloads it returned: {counters:?}"
    );
}
