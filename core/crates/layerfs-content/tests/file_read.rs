//! Logical file and range reads, including repeated demands of one payload.

mod support;

use layerfs_content::file::mapping::decode_file_state;
use layerfs_content::{read_all, read_range, ContentError, ObjectRole};
use support::{
    build_file, disabled_scope, mapping_page_sizes, noise, patterned, read_back, repeat,
    CountingStore, MemoryStore, PoisonedStore,
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

/// Navigation is grouped by level, not one point call per visited page.
///
/// The fixture carries a real two-level mapping tree, so the difference is
/// visible: every mapping page is still read exactly once, but the pages of one
/// level arrive in a single provider call (a level wave) instead of one call each.
#[test]
fn navigation_acquires_a_whole_level_per_provider_call() {
    use layerfs_content::file::mapping::READ_NAVIGATION_WAVE;
    let bytes = noise(4 * 1024 * 1024 + 17);
    let (store, constructed, _) = chunked(bytes.len());
    let pages = mapping_page_sizes(&store, constructed.root);
    assert!(
        pages.len() >= 3,
        "the fixture must carry a real mapping tree: {pages:?}"
    );
    let leaves = pages.iter().filter(|(level, _, _)| *level == 0).count() as u64;
    assert!(
        leaves >= 2,
        "the fixture must carry several leaves: {pages:?}"
    );

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
    .expect("read");
    assert_eq!(out, bytes);
    assert_eq!(
        counters.nodes_read as usize,
        pages.len(),
        "every mapping page is read exactly once: {counters:?}"
    );
    // Two levels: the root, then every leaf of the range in bounded level waves.
    let expected = 1 + leaves.div_ceil(READ_NAVIGATION_WAVE as u64);
    assert_eq!(
        counters.node_batches_read, expected,
        "one navigation call per level wave: {counters:?}"
    );
    assert!(
        counters.node_batches_read < counters.nodes_read,
        "navigation must not be one call per page: {counters:?}"
    );
    assert_eq!(counters.max_node_batch, leaves);
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

/// Provider that serves forged canonical bytes for named identities.
///
/// The forgery lives only in the provider: the base construction and every
/// object the read does not override stay exactly as they were built, so the case
/// exercises the read path against a corrupt root rather than against a corrupt
/// fixture.
struct ForgingStore<'a> {
    inner: &'a MemoryStore,
    overrides: Vec<(layerfs_content::ObjectId, Vec<u8>)>,
}

impl<'a> ForgingStore<'a> {
    fn new(inner: &'a MemoryStore, overrides: Vec<(layerfs_content::ObjectId, Vec<u8>)>) -> Self {
        Self { inner, overrides }
    }
}

impl layerfs_content::AuthenticatedObjects for ForgingStore<'_> {
    fn read_canonical_batch(
        &self,
        ids: &[layerfs_content::ObjectId],
    ) -> layerfs_content::ContentResult<Vec<Vec<u8>>> {
        ids.iter()
            .map(
                |id| match self.overrides.iter().find(|(named, _)| named == id) {
                    Some((_, bytes)) => Ok(bytes.clone()),
                    None => self
                        .inner
                        .canonical(*id)
                        .map(<[u8]>::to_vec)
                        .ok_or(layerfs_content::ContentError::MissingObject),
                },
            )
            .collect()
    }
}

/// Rewrites one big-endian `u64` field of a canonical bytes-role value.
fn patch_u64(canonical: &[u8], offset: usize, value: u64) -> Vec<u8> {
    let mut held = layerfs_content::object::codec::decode_bytes_object(canonical)
        .expect("canonical object")
        .to_vec();
    held[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
    layerfs_content::object::codec::encode_bytes_object(&held).expect("re-encoded object")
}

#[test]
fn a_state_that_overstates_its_mapping_is_refused_not_partly_served() {
    // A branch root whose own summary, and the file state that opens it, both
    // claim one more byte than the extents below actually cover. Every page stays
    // internally canonical, so nothing rejects the forgery at decode: the read is
    // the only place that can notice, and it must notice rather than return `Ok`
    // with a byte missing.
    // Repetitive input makes the frozen chunker emit one maximum-size chunk after
    // another, so a 5 MiB file needs more than one root page and the fixture has a
    // branch root by construction rather than by luck.
    let bytes = repeat(5 * 1024 * 1024 + 1, 0x33);
    let (store, constructed) = build_file(&bytes);
    let state = decode_file_state(store.canonical(constructed.root).unwrap()).unwrap();
    let root = store.canonical(state.mapping_root).unwrap();
    let level = match layerfs_content::file::mapping::decode_node(root).unwrap() {
        layerfs_content::file::mapping::ExtentNode::Branch { level, .. } => level,
        other => panic!(
            "this fixture needs a branch root, found level {}",
            other.level()
        ),
    };
    assert!(level >= 1);

    // The branch's total (value bytes 15..23) and its last child's cumulative
    // end (the last directory entry's first eight bytes) move together, and the
    // file state's logical length (value bytes 12..20) moves with them.
    let entries = match layerfs_content::file::mapping::decode_node(root).unwrap() {
        layerfs_content::file::mapping::ExtentNode::Branch { children, .. } => children.len(),
        _ => unreachable!(),
    };
    let last_entry = 31 + 48 * (entries - 1);
    let claimed = state.logical_len + 1;
    let forged_root = patch_u64(&patch_u64(root, 15, claimed), last_entry, claimed);
    let forged_state = patch_u64(store.canonical(constructed.root).unwrap(), 12, claimed);

    let provider = ForgingStore::new(
        &store,
        vec![
            (constructed.root, forged_state),
            (state.mapping_root, forged_root),
        ],
    );
    let mut out = Vec::new();
    let error = disabled_scope(|scope| {
        read_all(
            &provider,
            constructed.root,
            &mut out,
            scope.child("content.read"),
        )
    })
    .unwrap_err();
    assert!(
        matches!(error, ContentError::InvalidRecord("mapping coverage")),
        "an overstated state produced {error}"
    );
    assert_eq!(
        out.len(),
        bytes.len(),
        "the read emitted what the real tree covers, never a short result"
    );
}

#[test]
fn a_state_that_contradicts_its_root_page_is_refused_before_the_walk() {
    let bytes = noise(131_072 * 3);
    let (store, constructed) = build_file(&bytes);
    let state = decode_file_state(store.canonical(constructed.root).unwrap()).unwrap();
    let forged_state = patch_u64(
        store.canonical(constructed.root).unwrap(),
        12,
        state.logical_len + 1_024,
    );
    let provider = ForgingStore::new(&store, vec![(constructed.root, forged_state)]);
    let mut out = Vec::new();
    let error = disabled_scope(|scope| {
        read_all(
            &provider,
            constructed.root,
            &mut out,
            scope.child("content.read"),
        )
    })
    .unwrap_err();
    assert!(
        matches!(error, ContentError::InvalidRecord("mapping coverage")),
        "a state disagreeing with its root page produced {error}"
    );
}

#[test]
fn a_leaf_slice_outside_the_chunk_grammar_is_refused_at_decode() {
    // A slice that starts near the top of the `u32` space and runs past the
    // largest chunk the frozen grammar can produce. The old check only asked that
    // the two fields not overflow each other, so this page decoded "successfully"
    // and was refused much later, at the first read that sliced the payload. It is
    // now refused where it is decoded.
    let bytes = noise(131_072 * 2);
    let (store, constructed) = build_file(&bytes);
    let state = decode_file_state(store.canonical(constructed.root).unwrap()).unwrap();
    let root = store.canonical(state.mapping_root).unwrap();
    let (leaf_id, forged) = match layerfs_content::file::mapping::decode_node(root).unwrap() {
        layerfs_content::file::mapping::ExtentNode::Leaf { .. } => {
            let entry = 31;
            let mut held = layerfs_content::object::codec::decode_bytes_object(root)
                .unwrap()
                .to_vec();
            held[entry + 32..entry + 36].copy_from_slice(&65_520_u32.to_be_bytes());
            held[entry + 36..entry + 40].copy_from_slice(&32_u32.to_be_bytes());
            (
                state.mapping_root,
                layerfs_content::object::codec::encode_bytes_object(&held).unwrap(),
            )
        }
        layerfs_content::file::mapping::ExtentNode::Branch { children, .. } => {
            let id = children[0].child_object_id;
            let child = store.canonical(id).unwrap();
            let mut held = layerfs_content::object::codec::decode_bytes_object(child)
                .unwrap()
                .to_vec();
            held[31 + 32..31 + 36].copy_from_slice(&65_520_u32.to_be_bytes());
            held[31 + 36..31 + 40].copy_from_slice(&32_u32.to_be_bytes());
            (
                id,
                layerfs_content::object::codec::encode_bytes_object(&held).unwrap(),
            )
        }
    };
    let error = layerfs_content::file::mapping::decode_node(&forged).unwrap_err();
    assert!(
        matches!(error, ContentError::InvalidRecord("extent slice")),
        "an out-of-grammar slice produced {error}"
    );

    // Through a real read, the same forgery is refused by the decoder.
    let provider = ForgingStore::new(&store, vec![(leaf_id, forged)]);
    let mut out = Vec::new();
    let error = disabled_scope(|scope| {
        read_all(
            &provider,
            constructed.root,
            &mut out,
            scope.child("content.read"),
        )
    })
    .unwrap_err();
    assert!(
        matches!(error, ContentError::InvalidRecord("extent slice")),
        "the read produced {error}"
    );
}
