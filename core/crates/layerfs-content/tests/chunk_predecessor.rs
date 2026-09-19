//! The positional chunk correspondence: which stored payload a new chunk continues.
//!
//! A chunked version constructed against an earlier chunked version must offer each
//! of its chunks the base extent covering that chunk's own byte range, and a
//! construction with no base must be unchanged. The cases below check both against
//! the mapping trees the two constructions actually emitted.

mod support;

use std::collections::BTreeMap;

use layerfs_content::file::mapping::{decode_file_state, decode_node_with_context, ExtentNode};
use layerfs_content::{
    construct_bytes, construct_bytes_with_predecessor, ConstructionPolicy, ContentResult,
    FinalizedConsumer, FinalizedObject, ObjectId, PredecessorBase,
};
use support::{disabled_scope, noise, patterned, Counted, CountingProvider, MemoryStore};

/// Consumer that records every emitted object and the predecessors it carried.
#[derive(Default)]
struct RecordingConsumer {
    store: MemoryStore,
    /// "object -> proposed base identities", in preference order.
    predecessors: BTreeMap<ObjectId, Vec<ObjectId>>,
    /// Every emitted identity in emission order, with repeats.
    emitted: Vec<ObjectId>,
}

impl RecordingConsumer {
    fn new() -> Self {
        Self::default()
    }
}

impl FinalizedConsumer for RecordingConsumer {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.predecessors
            .insert(object.id(), object.predecessors().ids().collect());
        self.emitted.push(object.id());
        self.store.accept(object)
    }
}

/// "(start, length, payload)" of every extent reachable from a chunked root.
fn extents(store: &MemoryStore, root: ObjectId) -> Vec<(u64, u32, ObjectId)> {
    let canonical = store.canonical(root).expect("root bytes");
    let state = decode_file_state(canonical).expect("file state");
    let mut found = Vec::new();
    let mut stack = vec![(state.mapping_root, true, 0_u64)];
    while let Some((id, is_root, base)) = stack.pop() {
        let bytes = store.canonical(id).expect("node bytes");
        match decode_node_with_context(bytes, is_root).expect("canonical page") {
            ExtentNode::Leaf { extents, .. } => {
                let mut position = base;
                for extent in extents {
                    found.push((
                        position,
                        extent.logical_length(),
                        extent.payload_object_id(),
                    ));
                    position += u64::from(extent.logical_length());
                }
            }
            ExtentNode::Branch { children, .. } => {
                let mut previous = 0_u64;
                for child in children {
                    stack.push((child.child_object_id, false, base + previous));
                    previous = child.cumulative_logical_end;
                }
            }
        }
    }
    found.sort_by_key(|(start, _, _)| *start);
    found
}

fn policy() -> ConstructionPolicy {
    ConstructionPolicy::frozen_default()
}

fn build(bytes: &[u8]) -> (MemoryStore, ObjectId) {
    let mut consumer = RecordingConsumer::new();
    let constructed = disabled_scope(|scope| {
        construct_bytes(
            policy(),
            &policy().capacities(),
            bytes,
            &mut consumer,
            scope.child("content"),
        )
    })
    .expect("construction succeeds");
    (consumer.store, constructed.root)
}

fn build_with_base(
    bytes: &[u8],
    base: &MemoryStore,
    base_root: ObjectId,
) -> (MemoryStore, ObjectId, BTreeMap<ObjectId, Vec<ObjectId>>) {
    let mut consumer = RecordingConsumer::new();
    let constructed = disabled_scope(|scope| {
        construct_bytes_with_predecessor(
            policy(),
            &policy().capacities(),
            bytes,
            Some(PredecessorBase::new(base, base_root)),
            &mut consumer,
            scope.child("content"),
        )
    })
    .expect("construction succeeds");
    let predecessors = consumer.predecessors.clone();
    (consumer.store, constructed.root, predecessors)
}

/// A 600 KiB incompressible body with one small region rewritten, so the two
/// versions re-synchronise outside the edit and differ inside it.
fn edited_body() -> (Vec<u8>, Vec<u8>) {
    let base = noise(600_000);
    let mut edited = base.clone();
    edited[200_000..200_100].copy_from_slice(&patterned(100));
    (base, edited)
}

#[test]
fn every_chunk_is_offered_the_base_extent_covering_its_own_range() {
    let (base_bytes, edited_bytes) = edited_body();
    let (base_store, base_root) = build(&base_bytes);
    let base_extents = extents(&base_store, base_root);
    assert!(base_extents.len() > 8, "the base must have several extents");

    let (result_store, result_root, predecessors) =
        build_with_base(&edited_bytes, &base_store, base_root);
    let result_extents = extents(&result_store, result_root);
    assert_eq!(result_extents.len(), base_extents.len());

    let mut based = 0_usize;
    for (start, length, payload) in &result_extents {
        let end = start + u64::from(*length);
        let expected = base_extents
            .iter()
            .find(|(base_start, base_length, _)| {
                *base_start < end && base_start + u64::from(*base_length) > *start
            })
            .map(|(_, _, base_payload)| *base_payload);
        let offered = predecessors.get(payload).cloned().unwrap_or_default();
        match expected {
            Some(expected) => {
                assert_eq!(
                    offered,
                    vec![expected],
                    "the chunk at {start} must be offered the base extent covering it"
                );
                based += 1;
            }
            None => assert!(offered.is_empty(), "no base extent covers {start}"),
        }
    }
    assert_eq!(based, result_extents.len());
    // The rewritten region must have produced at least one new chunk, or the case
    // would only be proving that identical content is offered itself.
    assert!(
        result_extents
            .iter()
            .any(|(_, _, payload)| !base_extents.iter().any(|(_, _, base)| base == payload)),
        "the edit must change at least one chunk"
    );
}

#[test]
fn a_base_less_build_is_unchanged() {
    let bytes = noise(600_000);
    let mut plain = RecordingConsumer::new();
    let mut explicit = RecordingConsumer::new();
    let (plain_root, explicit_root) = disabled_scope(|scope| {
        let plain_root = construct_bytes(
            policy(),
            &policy().capacities(),
            &bytes,
            &mut plain,
            scope.child("content"),
        )?
        .root;
        let explicit_root = construct_bytes_with_predecessor(
            policy(),
            &policy().capacities(),
            &bytes,
            None,
            &mut explicit,
            scope.child("content"),
        )?
        .root;
        Ok::<_, layerfs_content::ContentError>((plain_root, explicit_root))
    })
    .expect("both constructions succeed");
    assert_eq!(plain_root, explicit_root);
    assert_eq!(plain.store.order(), explicit.store.order());
    for (id, _) in plain.store.order() {
        assert_eq!(plain.store.canonical(*id), explicit.store.canonical(*id));
    }
    assert!(
        plain.predecessors.values().all(Vec::is_empty),
        "a construction with no base proposes no predecessor"
    );
}

#[test]
fn an_identical_version_is_offered_its_own_chunks() {
    let bytes = noise(400_000);
    let (base_store, base_root) = build(&bytes);
    let (_, _, predecessors) = build_with_base(&bytes, &base_store, base_root);
    let base_extents = extents(&base_store, base_root);
    for (_, _, payload) in &base_extents {
        assert_eq!(
            predecessors.get(payload).cloned().unwrap_or_default(),
            vec![*payload]
        );
    }
}

#[test]
fn a_whole_file_base_offers_nothing() {
    let (base_store, base_root) = build(&patterned(1_000));
    let bytes = noise(400_000);
    let (_, _, predecessors) = build_with_base(&bytes, &base_store, base_root);
    assert!(
        predecessors.values().all(Vec::is_empty),
        "a whole-file base is not admitted for a chunk"
    );
}

#[test]
fn the_correspondence_reads_mapping_pages_and_no_payload() {
    let (base_bytes, edited_bytes) = edited_body();
    let (base_store, base_root) = build(&base_bytes);
    let base_state =
        decode_file_state(base_store.canonical(base_root).expect("root")).expect("state");
    let payload_ids: Vec<ObjectId> = extents(&base_store, base_root)
        .into_iter()
        .map(|(_, _, payload)| payload)
        .collect();

    let counts = CountingProvider::new();
    let counted = Counted {
        store: &base_store,
        counts: &counts,
    };
    let mut consumer = RecordingConsumer::new();
    disabled_scope(|scope| {
        construct_bytes_with_predecessor(
            policy(),
            &policy().capacities(),
            &edited_bytes,
            Some(PredecessorBase::new(&counted, base_root)),
            &mut consumer,
            scope.child("content"),
        )
    })
    .expect("construction succeeds");

    let demanded = counts.ids.borrow().clone();
    assert!(!demanded.is_empty(), "the cursor must read the base");
    assert_eq!(demanded[0], base_root, "the first read is the base root");
    for id in &demanded {
        assert!(
            !payload_ids.contains(id),
            "the cursor read a chunk payload instead of a mapping page"
        );
    }
    // One read of the base root and one of the mapping root: a 600 KiB base is a
    // single extent leaf, so the correspondence costs two pages.
    assert_eq!(demanded, vec![base_root, base_state.mapping_root]);
    assert_eq!(counts.batch_calls(), 2);
}

#[test]
fn a_base_the_provider_cannot_serve_is_a_refusal() {
    let (base_bytes, edited_bytes) = edited_body();
    let (base_store, base_root) = build(&base_bytes);
    assert!(!base_store.is_empty());
    let mut empty = MemoryStore::new();
    assert!(!empty.remove(base_root));
    let mut consumer = RecordingConsumer::new();
    let error = disabled_scope(|scope| {
        construct_bytes_with_predecessor(
            policy(),
            &policy().capacities(),
            &edited_bytes,
            Some(PredecessorBase::new(&empty, base_root)),
            &mut consumer,
            scope.child("content"),
        )
    })
    .expect_err("an unservable base is refused");
    assert_eq!(error, layerfs_content::ContentError::MissingObject);
}

/// A base whose mapping is a **branch**, edited in place and then grown.
///
/// `MAX_ENTRIES = 128` extents fit on one page, so a 3.5 MiB incompressible body
/// overflows a leaf and its root becomes a branch. This is the only case that
/// exercises the cursor's descent - child bases, child summaries and the subtree
/// skip - and the appended tail is the only case that exercises a chunk past the
/// end of the base, which must be offered nothing.
#[test]
fn a_branched_base_mapping_is_walked_and_a_grown_result_stops_at_its_end() {
    let base = noise(3_500_000);
    let mut grown = base.clone();
    grown[1_000_000..1_000_100].copy_from_slice(&patterned(100));
    grown.extend_from_slice(&noise(60_000));

    let (base_store, base_root) = build(&base);
    let state = decode_file_state(base_store.canonical(base_root).expect("root")).expect("state");
    assert!(state.tree_level >= 1, "the base mapping must be a branch");
    let base_extents = extents(&base_store, base_root);
    assert!(base_extents.len() > 128, "the base must overflow one page");

    let (result_store, result_root, predecessors) = build_with_base(&grown, &base_store, base_root);
    let result_extents = extents(&result_store, result_root);
    assert!(result_extents.len() > base_extents.len());
    let mut uncovered = 0_usize;
    for (start, length, payload) in &result_extents {
        let end = start + u64::from(*length);
        let expected = base_extents
            .iter()
            .find(|(base_start, base_length, _)| {
                *base_start < end && base_start + u64::from(*base_length) > *start
            })
            .map(|(_, _, base_payload)| *base_payload);
        if expected.is_none() {
            uncovered += 1;
        }
        assert_eq!(
            predecessors.get(payload).cloned().unwrap_or_default(),
            expected.map(|id| vec![id]).unwrap_or_default(),
            "the chunk at {start} of a branched base"
        );
    }
    assert!(
        uncovered > 0,
        "the appended tail must lie past the base's own mapping"
    );
}
