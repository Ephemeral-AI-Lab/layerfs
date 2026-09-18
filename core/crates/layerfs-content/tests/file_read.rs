//! Logical file and range reads, including repeated demands of one payload.

mod support;

use layerfs_content::file::mapping::{decode_file_state, emit_file_state, ExtentBuilder};
use layerfs_content::{read_all, read_range, ConstructionPolicy, ContentError, ObjectRole};
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

/// The payload wave refuses an object larger than the chunk grammar allows.
///
/// A wave declares both an object count and a byte ceiling. The count was
/// enforced and the byte figure was not: a provider that served an oversized
/// object under a chunk role had its payload sliced and copied out, so the row
/// that quoted `READ_WAVE_BYTES` described a bound nothing checked. The object
/// here is canonical and correctly identified - it is refused because of its
/// **size**, not because its bytes do not hash to its id - so the refusal cannot
/// be confused with the identity check. Its extent restates four bytes, so a
/// conforming object would be legal; the provider below hands back a chunk far
/// past the chunk maximum, which is exactly the case the byte ceiling exists for.
/// The page encoder validates the context the page will be read in.
///
/// The encoder used to validate every page it was given as a **root**, so a short
/// page emitted for a non-root position was publishable and then unreadable: the
/// reader refuses a non-root page below the canonical partition minimum. The
/// encoder now takes the context from the caller that decides which page is the
/// tree's root, and this case pins both sides of it - the same short page is
/// accepted as a root and refused as a child.
#[test]
fn a_short_page_is_accepted_as_a_root_and_refused_as_a_non_root() {
    use layerfs_content::file::mapping::{encode_node, ExtentNode, ExtentSlice, MIN_ENTRIES};
    use layerfs_content::ObjectId;

    let short = ExtentNode::Leaf {
        subtree_logical_bytes: 0,
        extents: Vec::new(),
    };
    assert!(
        encode_node(&short, true).is_ok(),
        "an empty leaf is the canonical root of an empty mapping"
    );
    let outcome = encode_node(&short, false);
    assert!(
        matches!(outcome, Err(ContentError::NonCanonicalPagePartition)),
        "a page below the partition minimum is refused in child context: {outcome:?}"
    );

    // The boundary itself: one entry short is refused, the minimum is accepted.
    let page = |count: usize| ExtentNode::Leaf {
        subtree_logical_bytes: (count * 4) as u64,
        extents: (0..count)
            .map(|index| {
                ExtentSlice::new(ObjectId::for_bytes(&(index as u32).to_be_bytes()), 0, 4)
                    .expect("four-byte extent")
            })
            .collect(),
    };
    assert!(matches!(
        encode_node(&page(MIN_ENTRIES - 1), false),
        Err(ContentError::NonCanonicalPagePartition)
    ));
    assert!(
        encode_node(&page(MIN_ENTRIES), false).is_ok(),
        "a page at the partition minimum is a legal child"
    );
}

#[test]
fn the_payload_wave_enforces_both_its_object_and_its_byte_ceiling() {
    use layerfs_content::file::mapping::{
        encode_chunk_object, encode_file_state, encode_node, profile_id, ExtentNode, ExtentSlice,
        FileState, READ_WAVE_BYTES, READ_WAVE_OBJECTS,
    };
    use layerfs_content::{
        AuthenticatedObjects, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
        ObjectRole,
    };

    /// Serves the real leaf and state, and one crafted payload under them.
    struct CraftedChunk {
        inner: MemoryStore,
        payload_id: ObjectId,
        canonical: Vec<u8>,
    }

    impl AuthenticatedObjects for CraftedChunk {
        fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
            ids.iter()
                .map(|id| {
                    if *id == self.payload_id {
                        return Ok(self.canonical.clone());
                    }
                    self.inner.read_canonical(*id)
                })
                .collect()
        }
    }

    let oversize = layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES + 1_024;
    // Hand-built canonical chunk: the object envelope, a 4-byte payload length and
    // the chunk value header, exactly as the public encoder writes them, but with a
    // payload the public encoder refuses to produce.
    let mut value = Vec::new();
    value.extend_from_slice(b"LFS4CHK\0");
    value.extend_from_slice(&vec![0x6b_u8; oversize]);
    let mut canonical = Vec::new();
    canonical.extend_from_slice(b"LFSO");
    canonical.push(1);
    canonical.extend_from_slice(&((value.len() + 4) as u32).to_be_bytes());
    canonical.extend_from_slice(&(value.len() as u32).to_be_bytes());
    canonical.extend_from_slice(&value);

    let payload_id = ObjectId::for_bytes(&canonical);
    let leaf = FinalizedObject::new(
        ObjectRole::ExtentLeaf,
        encode_node(
            &ExtentNode::Leaf {
                subtree_logical_bytes: 4,
                extents: vec![ExtentSlice::new(payload_id, 0, 4).expect("four-byte extent")],
            },
            true,
        )
        .expect("leaf"),
    )
    .expect("canonical leaf")
    .with_references(vec![payload_id]);
    let leaf_id = leaf.id();
    let state = FinalizedObject::new(
        ObjectRole::FileState,
        encode_file_state(FileState {
            logical_len: 4,
            extent_count: 1,
            tree_level: 0,
            profile_id: profile_id(),
            mapping_root: leaf_id,
        })
        .expect("state"),
    )
    .expect("canonical state")
    .with_references(vec![leaf_id]);
    let root = state.id();

    let mut inner = MemoryStore::new();
    inner.accept(leaf).expect("the leaf is held");
    inner.accept(state).expect("the state is held");
    let store = CraftedChunk {
        inner,
        payload_id,
        canonical,
    };

    let mut out = Vec::new();
    let outcome = disabled_scope(|scope| {
        read_range(&store, root, 0..4, &mut out, scope.child("content.read"))
    });
    assert!(
        matches!(
            outcome,
            Err(ContentError::ObjectLimitExceeded { limit, actual })
                if limit == layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES
                    && actual == oversize
        ),
        "an object above the chunk maximum is refused at the wave boundary: {outcome:?}"
    );
    assert!(
        out.is_empty(),
        "a refused wave emits no bytes: {}",
        out.len()
    );

    // The byte ceiling's arithmetic, locked: one wave holds at most
    // `READ_WAVE_OBJECTS` payloads and each is at most one chunk, so the largest
    // legal wave is exactly `READ_WAVE_BYTES`. Moving either constant without the
    // other would leave the byte figure describing a bound the object count
    // already refuses, and this assertion fails instead.
    assert_eq!(
        READ_WAVE_BYTES,
        READ_WAVE_OBJECTS * layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES
    );
    let chunk = layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES;
    let mut store = MemoryStore::new();
    let mut extents = Vec::new();
    for index in 0..READ_WAVE_OBJECTS {
        let bytes = vec![index as u8; chunk];
        let canonical = encode_chunk_object(&bytes).expect("a maximum-sized chunk is legal");
        let object = FinalizedObject::new(ObjectRole::Chunk, canonical).expect("canonical chunk");
        let id = object.id();
        store.accept(object).expect("the provider holds it");
        extents.push(ExtentSlice::new(id, 0, chunk as u32).expect("extent at the maximum"));
    }
    let total = READ_WAVE_OBJECTS * chunk;
    let leaf = FinalizedObject::new(
        ObjectRole::ExtentLeaf,
        encode_node(
            &ExtentNode::Leaf {
                subtree_logical_bytes: total as u64,
                extents,
            },
            true,
        )
        .expect("a legal leaf"),
    )
    .expect("canonical leaf");
    let leaf_id = leaf.id();
    store.accept(leaf).expect("the provider holds it");
    let state = FinalizedObject::new(
        ObjectRole::FileState,
        encode_file_state(FileState {
            logical_len: total as u64,
            extent_count: READ_WAVE_OBJECTS as u64,
            tree_level: 0,
            profile_id: profile_id(),
            mapping_root: leaf_id,
        })
        .expect("state"),
    )
    .expect("canonical state")
    .with_references(vec![leaf_id]);
    let root = state.id();
    store.accept(state).expect("the provider holds it");

    let mut out = Vec::new();
    let counters = disabled_scope(|scope| {
        read_range(
            &store,
            root,
            0..total as u64,
            &mut out,
            scope.child("content.read"),
        )
    })
    .expect("the largest legal wave is served");
    assert_eq!(out.len(), total);
    assert_eq!(
        counters.max_payload_batch as usize, READ_WAVE_OBJECTS,
        "one wave holds exactly the object ceiling at the chunk maximum: {counters:?}"
    );
    assert!(
        counters.payload_bytes_read as usize <= READ_WAVE_BYTES,
        "the bytes one read serves stay inside the declared ceiling: {counters:?}"
    );
}

/// A range read serves a tree with more than one branch level.
///
/// `ChildDescriptor::cumulative_logical_end` is cumulative *within its own page*,
/// not from the start of the file. A traversal that compares it against an
/// absolute range bound therefore prunes every child of a non-root branch whose
/// subtree begins past the range, emits fewer bytes than it was asked for, and
/// refuses itself with `InvalidRecord("mapping coverage")`. A tree whose root is
/// a single branch level hides this, because there the page origin is zero and the
/// two readings of the field agree. With the frozen page capacity of 128 entries
/// the tree gains its second branch level at 16,385 extents.
#[test]
fn a_range_read_serves_a_tree_with_more_than_one_branch_level() {
    let policy = ConstructionPolicy::frozen_default();
    let extents = 16_385_usize;
    let mut store = MemoryStore::new();
    let mut builder = ExtentBuilder::new(&policy.capacities());
    let expected: Vec<u8> = (0..extents).map(|index| (index % 251) as u8).collect();
    for byte in &expected {
        builder
            .push_chunk(std::slice::from_ref(byte), None, &mut store)
            .expect("one chunk per logical byte");
    }
    let build = builder.finish(&mut store).expect("mapping tree");
    let root = emit_file_state(&mut store, build.root.expect("mapping root")).expect("file state");
    let state = decode_file_state(store.canonical(root).unwrap()).expect("decoded file state");
    assert_eq!(
        state.tree_level, 2,
        "the tree has two branch levels above its leaves"
    );
    assert_eq!(state.extent_count, extents as u64);
    assert_eq!(state.logical_len, extents as u64);

    // A range that starts in the first leaf is the case the old traversal got
    // right; the middle and tail ranges are the ones it pruned away.
    let ranges = [
        0_u64..1_000,
        (extents as u64 / 2)..(extents as u64 / 2 + 1_000),
        (extents as u64 - 4_096)..(extents as u64),
    ];
    for range in ranges {
        let mut out = Vec::new();
        disabled_scope(|scope| {
            read_range(
                &store,
                root,
                range.clone(),
                &mut out,
                scope.child("content.read"),
            )
        })
        .unwrap_or_else(|error| panic!("range {range:?} failed: {error}"));
        assert_eq!(
            out,
            expected[range.start as usize..range.end as usize],
            "range {range:?} is served byte-exactly"
        );
    }
}
