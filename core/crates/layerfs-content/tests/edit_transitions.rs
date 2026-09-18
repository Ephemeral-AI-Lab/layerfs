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

// ---------------------------------------------------------------------------
// Physical read amplification on the representation transition (#169 item 2).
// ---------------------------------------------------------------------------
//
// **The accounted quantity, defined before measuring.** A transition's physical
// read amplification is the canonical bytes the operation acquires from its
// supplied provider, divided by the logical bytes the transition produces. Every
// read this component performs goes through `AuthenticatedObjects`, so the
// provider boundary is the one place that sees all of them, across all three
// result representations. `EditCounters` charges the stored-tree walk on the
// chunked lane and is reported beside it; it is *unavailable*, not zero, on the
// two paths that publish no counters (whole-file assembly and the empty result),
// and those rows say so.
//
// Two independent models are used, and neither is the product's own read path:
//
// * a **coverage model**, which decodes the base's mapping tree through the public
//   codec and records, for every payload object, the logical ranges it covers.
//   That is what makes "a discarded range is never read" checkable: the discarded
//   interval is known, and the objects that lie *entirely* inside it are known.
// * an **independent construction**, which builds the expected final bytes from
//   scratch and must reach the same root the transition did.
//
// **Declared bounds**, asserted rather than described. `MAX_NODE_BYTES` is the
// largest canonical mapping page; `MAX_CHUNK` the largest chunk the frozen
// grammar can produce.
//
// | case | declared bound on payload bytes acquired |
// | --- | --- |
// | small -> large | the whole single-object base (nothing is discarded) |
// | large -> small | `final + 2 * MAX_CHUNK` (the kept ranges plus one straddling chunk per edge) |
// | any -> empty | `0` |
// | in-place overwrite | `4 * MAX_CHUNK`, and strictly under half the base |
//
// Every case additionally asserts that no object lying entirely inside the
// discarded interval was acquired, that the base was acquired through the
// supplied provider and not re-read whole, and that the acquisition is charged
// (`EditCounters` where the path publishes them, the census everywhere).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use layerfs_content::file::mapping::{decode_node_with_context, ExtentNode};
use layerfs_content::file::EditCounters;
use layerfs_content::{AuthenticatedObjects, ConstructedFile, ContentResult};

/// Largest canonical mapping page: the declared per-node allowance.
const MAX_NODE_BYTES: u64 = layerfs_content::file::mapping::MAX_NODE_OBJECT_BYTES as u64;
/// Largest chunk payload the frozen grammar can produce.
const MAX_CHUNK: u64 = layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES as u64;

/// Provider that records every acquisition it serves.
struct Census<'a> {
    inner: &'a MemoryStore,
    log: RefCell<Vec<(ObjectId, u64)>>,
}

impl<'a> Census<'a> {
    fn new(inner: &'a MemoryStore) -> Self {
        Self {
            inner,
            log: RefCell::new(Vec::new()),
        }
    }

    fn acquisitions(&self) -> Vec<(ObjectId, u64)> {
        self.log.borrow().clone()
    }
}

impl AuthenticatedObjects for Census<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let values = self.inner.read_canonical_batch(ids)?;
        let mut log = self.log.borrow_mut();
        for (id, bytes) in ids.iter().zip(&values) {
            log.push((*id, bytes.len() as u64));
        }
        Ok(values)
    }
}

/// Logical coverage of every payload object the base's tree reaches.
fn coverage(store: &MemoryStore, root: ObjectId) -> BTreeMap<ObjectId, Vec<Range<u64>>> {
    let mut map: BTreeMap<ObjectId, Vec<Range<u64>>> = BTreeMap::new();
    match representation(store, root) {
        FileContent::WholeFile { logical_len } => {
            map.entry(root).or_default().push(0..logical_len);
        }
        FileContent::Chunked(state) => {
            walk_coverage(
                store,
                state.mapping_root,
                0,
                true,
                state.tree_level,
                &mut map,
            );
        }
    }
    map
}

fn walk_coverage(
    store: &MemoryStore,
    id: ObjectId,
    origin: u64,
    root: bool,
    level: u8,
    map: &mut BTreeMap<ObjectId, Vec<Range<u64>>>,
) {
    let node = decode_node_with_context(store.canonical(id).expect("stored page"), root)
        .expect("canonical page");
    assert_eq!(node.level(), level, "the base tree is consistent");
    match node {
        ExtentNode::Leaf { extents, .. } => {
            let mut position = origin;
            for extent in extents {
                let end = position + u64::from(extent.logical_length());
                map.entry(extent.payload_object_id())
                    .or_default()
                    .push(position..end);
                position = end;
            }
        }
        ExtentNode::Branch { children, .. } => {
            let mut previous = origin;
            for child in children {
                walk_coverage(
                    store,
                    child.child_object_id,
                    previous,
                    false,
                    level - 1,
                    map,
                );
                previous = child.cumulative_logical_end;
            }
        }
    }
}

/// Every object identity the base's tree names, pages included.
fn tree_objects(store: &MemoryStore, root: ObjectId) -> BTreeSet<ObjectId> {
    let mut ids = BTreeSet::new();
    ids.insert(root);
    if let FileContent::Chunked(state) = representation(store, root) {
        ids.insert(state.mapping_root);
        collect_pages(store, state.mapping_root, true, &mut ids);
    }
    ids
}

fn collect_pages(store: &MemoryStore, id: ObjectId, root: bool, ids: &mut BTreeSet<ObjectId>) {
    ids.insert(id);
    let node =
        decode_node_with_context(store.canonical(id).expect("stored page"), root).expect("page");
    if let ExtentNode::Branch { children, .. } = node {
        for child in children {
            collect_pages(store, child.child_object_id, false, ids);
        }
    }
}

/// One measured transition.
struct Measured {
    final_objects: BTreeSet<ObjectId>,
    acquired: Vec<(ObjectId, u64)>,
    counters: EditCounters,
    payload_acquired: u64,
    node_acquired: u64,
    payload_objects: BTreeSet<ObjectId>,
    exclusive_discarded: BTreeSet<ObjectId>,
    base_payload_bytes: u64,
    final_len: u64,
}

/// Runs one transition through the supplied provider and charges what it read.
#[allow(clippy::too_many_arguments)]
fn measure(
    policy: ConstructionPolicy,
    store: &MemoryStore,
    root: ObjectId,
    edits: Vec<Edit>,
    replacements: &Replacements,
    final_len: u64,
    discarded: Option<Range<u64>>,
) -> (MemoryStore, ObjectId, Measured) {
    let coverage = coverage(store, root);
    let base_payload_bytes: u64 = coverage
        .keys()
        .map(|id| store.canonical(*id).map(<[u8]>::len).unwrap_or(0) as u64)
        .sum();
    let exclusive_discarded: BTreeSet<ObjectId> = match &discarded {
        Some(interval) => coverage
            .iter()
            .filter(|(_, ranges)| {
                ranges
                    .iter()
                    .all(|range| range.start >= interval.start && range.end <= interval.end)
            })
            .map(|(id, _)| *id)
            .collect(),
        None => BTreeSet::new(),
    };

    let census = Census::new(store);
    let stream = EditStream::new(store.logical_len_probe(root), edits).expect("valid stream");
    assert_eq!(stream.final_len(), final_len);
    let mut result = store.merged_clone();
    let emitted_before = result.order().len();
    let constructed: ConstructedFile = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &census,
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
    let acquired = census.acquisitions();
    // Only what this transition emitted, never the base it started from.
    let final_objects: BTreeSet<ObjectId> = result.order()[emitted_before..]
        .iter()
        .map(|(id, _)| *id)
        .collect();
    let payload_acquired: u64 = acquired
        .iter()
        .filter(|(id, _)| coverage.contains_key(id))
        .map(|(_, bytes)| *bytes)
        .sum();
    let node_acquired: u64 = acquired
        .iter()
        .filter(|(id, _)| !coverage.contains_key(id))
        .map(|(_, bytes)| *bytes)
        .sum();
    let counters = constructed.counters;
    (
        result,
        constructed.root,
        Measured {
            final_objects,
            acquired,
            counters,
            payload_acquired,
            node_acquired,
            payload_objects: coverage.keys().copied().collect(),
            exclusive_discarded,
            base_payload_bytes,
            final_len,
        },
    )
}

impl Measured {
    /// Asserts the acquisition invariants every case shares.
    fn assert_charge(&self, store: &MemoryStore, base_root: ObjectId, label: &str) {
        let distinct = self
            .acquired
            .iter()
            .map(|(id, _)| *id)
            .collect::<BTreeSet<_>>();
        let mut base_objects = tree_objects(store, base_root);
        base_objects.extend(coverage(store, base_root).keys().copied());
        assert!(
            distinct.is_subset(&base_objects),
            "{label}: the base was not acquired through the supplied provider"
        );
        assert!(
            distinct.is_disjoint(&self.final_objects),
            "{label}: the transition acquired one of its own results"
        );
        for id in &self.exclusive_discarded {
            assert!(
                !distinct.contains(id),
                "{label}: an object lying entirely inside the discarded range was read"
            );
        }
        // The allowance is per *acquired page*, not per counted node: the two
        // paths that assemble a whole-file result publish no counters at all, so
        // `nodes_read` is unavailable there while the pages are still real.
        let page_objects = distinct
            .iter()
            .filter(|id| !self.payload_objects.contains(*id))
            .count();
        assert!(
            self.node_acquired <= page_objects as u64 * MAX_NODE_BYTES,
            "{label}: {} node bytes exceed the {page_objects}-page allowance",
            self.node_acquired
        );
        assert!(
            !self.acquired.is_empty(),
            "{label}: the transition acquired nothing at all"
        );
        println!(
            "MEASURED read-amplification case={label} base_payload_bytes={} acquired_bytes={} \
             acquired_payload_bytes={} acquired_node_bytes={} acquired_objects={} \
             exclusive_discarded={} final_bytes={} nodes_read={} nodes_created={} \
             payloads_created={} peak_deferred_bytes={} amplification_milli={}",
            self.base_payload_bytes,
            self.payload_acquired + self.node_acquired,
            self.payload_acquired,
            self.node_acquired,
            distinct.len(),
            self.exclusive_discarded.len(),
            self.final_len,
            self.counters.nodes_read,
            self.counters.nodes_created,
            self.counters.payloads_created,
            self.counters.peak_deferred_bytes,
            if self.final_len == 0 {
                "null".to_string()
            } else {
                ((self.payload_acquired + self.node_acquired) * 1_000 / self.final_len).to_string()
            }
        );
    }
}

/// Base bytes, the deletions and the expected result of the retained-run fixture.
///
/// A chunked base with a two-level mapping tree whose result is one whole-file
/// object, reached through three retained runs: the head, the middle run and the
/// tail. Each deletion discards its range without replacing it, so every retained
/// run is its own `Segment::Retain` and the whole-file arm assembles them: an
/// assembly that descends from the mapping root once per run pays for that root
/// three times. The kept ranges are the source of truth; the deletions between
/// them are derived in current-result coordinates, which is why each range is
/// shifted by what the earlier deletions removed.
const RETAINED_BASE: usize = 3_000_000;
const RETAINED_KEEP: [(u64, u64); 3] =
    [(0, 40_000), (1_500_000, 1_540_000), (2_960_000, 3_000_000)];

/// Base bytes, the deletions and the expected result of the retained-run fixture.
fn retained_segment_fixture(policy: ConstructionPolicy) -> (Vec<u8>, Vec<Edit>, Vec<u8>) {
    let base = noise(RETAINED_BASE);
    let mut expected = base.clone();
    let mut edits = Vec::new();
    let mut cursor = 0_u64;
    let mut removed = 0_u64;
    for (start, end) in RETAINED_KEEP
        .iter()
        .copied()
        .chain([(RETAINED_BASE as u64, RETAINED_BASE as u64)])
    {
        if start > cursor {
            edits.push(Edit::delete(cursor - removed, start - removed));
            expected.drain(cursor as usize - removed as usize..start as usize - removed as usize);
            removed += start - cursor;
        }
        cursor = end;
    }
    assert!(
        (expected.len() as u64) < policy.small_file_threshold_bytes(),
        "the fixture must assemble a whole-file result"
    );
    (base, edits, expected)
}

/// Mapping page identities of a chunked base, in logical order.
fn tree_pages(store: &MemoryStore, root: ObjectId) -> Vec<ObjectId> {
    let mut pages = vec![root];
    if let FileContent::Chunked(state) = representation(store, root) {
        pages.push(state.mapping_root);
        collect_nodes(store, state.mapping_root, true, &mut pages);
    }
    pages
}

fn collect_nodes(store: &MemoryStore, id: ObjectId, root: bool, pages: &mut Vec<ObjectId>) {
    if let ExtentNode::Branch { children, .. } =
        decode_node_with_context(store.canonical(id).expect("stored page"), root).expect("page")
    {
        for child in children {
            pages.push(child.child_object_id);
            collect_nodes(store, child.child_object_id, false, pages);
        }
    }
}

/// One retained run pays for a mapping page once for the whole assembly.
///
/// The three runs of the fixture sit on different mapping leaves, so an assembly
/// that descends from the mapping root once per run demands that root three times.
/// The ordered cursor demands it once: a page shared by several ascending ranges of
/// one operation is one demand, which is the whole difference the item buys.
#[test]
fn retained_segments_share_one_descent() {
    let policy = ConstructionPolicy::frozen_default();
    let (base, edits, expected) = retained_segment_fixture(policy);
    let (store, root) = build(policy, &base);
    let replacements = Replacements::new();
    let (result, edited_root, measured) = measure(
        policy,
        &store,
        root,
        edits,
        &replacements,
        expected.len() as u64,
        None,
    );
    assert_eq!(read_back(&result, edited_root).expect("read"), expected);
    let demands: Vec<ObjectId> = measured.acquired.iter().map(|(id, _)| *id).collect();
    let pages = tree_pages(&store, root);
    let mut page_demands: Vec<(ObjectId, usize)> = Vec::new();
    for page in &pages {
        let count = demands.iter().filter(|id| *id == page).count();
        page_demands.push((*page, count));
    }
    assert_eq!(
        page_demands[1].1, 1,
        "the mapping root is demanded once for the whole assembly: {page_demands:?}"
    );
    assert!(
        page_demands.len() > 2,
        "the fixture must carry a multi-page mapping tree: {page_demands:?}"
    );
    // Every mapping page is one demand, and each retained run is a distinct leaf,
    // so the demand count is the size of the union of the runs' paths - the head,
    // the middle run and the tail do not share one leaf.
    for (page, count) in &page_demands {
        assert!(
            *count <= 1,
            "mapping page {page} was demanded {count} times: {page_demands:?}"
        );
    }
    let file_state = root;
    assert_eq!(
        demands.iter().filter(|id| **id == file_state).count(),
        1,
        "the file state is demanded once"
    );
}

/// A payload two retained runs both reach is demanded once, not once per run.
///
/// The ranges are consecutive retained runs of one assembly, so a payload that
/// straddles the boundary between them is reached by both. The demand census is
/// per object identity, so the assertion is exact: every payload of the base is
/// demanded once for the whole assembly, whatever the run boundaries do to its
/// slices.
#[test]
fn straddling_payload_demanded_once_across_segments() {
    let policy = ConstructionPolicy::frozen_default();
    let (base, edits, expected) = retained_segment_fixture(policy);
    let (store, root) = build(policy, &base);
    let replacements = Replacements::new();
    let (result, edited_root, measured) = measure(
        policy,
        &store,
        root,
        edits,
        &replacements,
        expected.len() as u64,
        None,
    );
    assert_eq!(read_back(&result, edited_root).expect("read"), expected);
    let demands: Vec<ObjectId> = measured.acquired.iter().map(|(id, _)| *id).collect();
    let payloads = coverage(&store, root);
    // A payload the runs between them reach more than once would appear twice.
    let mut straddling = 0_usize;
    for id in payloads.keys() {
        let count = demands.iter().filter(|candidate| *candidate == id).count();
        assert!(
            count <= 1,
            "payload {id} was demanded {count} times by one assembly"
        );
        if count == 1 {
            straddling += 1;
        }
    }
    assert!(
        straddling > 3,
        "the fixture must demand several payloads: {straddling}"
    );
}

#[test]
fn the_transition_reads_only_what_it_keeps_and_charges_it() {
    for cutoff in CUTOFFS {
        let policy = policy_for(cutoff);
        let small_len = (cutoff / 2) as usize;
        let large_len = (4 * cutoff) as usize;

        // Small -> large: an insertion that crosses the cutoff. The base is one
        // whole-file object and the transition must read all of it; there is
        // nothing to skip, and the bound says so.
        {
            let base = patterned(small_len);
            let (store, root) = build(policy, &base);
            let addition = noise(cutoff as usize);
            let mut expected = base.clone();
            expected.splice(1_000..1_000, addition.iter().copied());
            let mut replacements = Replacements::new();
            replacements.push(addition.clone());
            let (result, edited_root, measured) = measure(
                policy,
                &store,
                root,
                vec![Edit::insert(1_000, addition.len() as u64)],
                &replacements,
                expected.len() as u64,
                None,
            );
            assert_eq!(read_back(&result, edited_root).expect("read"), expected);
            let (_, fresh) = build(policy, &expected);
            assert_eq!(edited_root, fresh, "small -> large at {cutoff}");
            assert_eq!(
                measured.base_payload_bytes,
                store.canonical(root).expect("base object").len() as u64,
                "the whole-file base is one payload"
            );
            assert!(
                measured.payload_acquired <= measured.base_payload_bytes,
                "small -> large at {cutoff} read more than its whole base"
            );
            assert_eq!(
                measured.exclusive_discarded.len(),
                0,
                "an insertion discards nothing"
            );
            measured.assert_charge(&store, root, &format!("small-to-large-{cutoff}"));
        }

        // Large -> small: a deletion that drops below the cutoff. The result is one
        // whole-file object built from the two kept ends, so the discarded middle
        // is never acquired and the payload bound is the final length plus one
        // straddling chunk per edge.
        {
            let base = noise(large_len);
            let (store, root) = build(policy, &base);
            let keep = cutoff / 4;
            let mut expected = base[..keep as usize].to_vec();
            expected.extend_from_slice(&base[base.len() - keep as usize..]);
            let replacements = Replacements::new();
            let (result, edited_root, measured) = measure(
                policy,
                &store,
                root,
                vec![Edit::delete(keep, large_len as u64 - keep)],
                &replacements,
                expected.len() as u64,
                Some(keep..large_len as u64 - keep),
            );
            assert_eq!(read_back(&result, edited_root).expect("read"), expected);
            let (_, fresh) = build(policy, &expected);
            assert_eq!(edited_root, fresh, "large -> small at {cutoff}");
            assert!(
                !measured.exclusive_discarded.is_empty(),
                "the fixture must discard whole payloads at {cutoff}"
            );
            assert!(
                measured.payload_acquired <= measured.final_len + 2 * MAX_CHUNK,
                "large -> small at {cutoff} acquired {} payload bytes for a {} byte result",
                measured.payload_acquired,
                measured.final_len
            );
            assert!(
                measured.payload_acquired * 4 <= measured.base_payload_bytes,
                "large -> small at {cutoff} acquired {} of {} base payload bytes",
                measured.payload_acquired,
                measured.base_payload_bytes
            );
            measured.assert_charge(&store, root, &format!("large-to-small-{cutoff}"));
        }

        // Any -> empty: a complete deletion acquires the file state and nothing
        // else. No payload is read and no mapping page is needed.
        {
            let base = noise(large_len);
            let (store, root) = build(policy, &base);
            let replacements = Replacements::new();
            let (result, edited_root, measured) = measure(
                policy,
                &store,
                root,
                vec![Edit::delete(0, large_len as u64)],
                &replacements,
                0,
                Some(0..large_len as u64),
            );
            assert!(read_back(&result, edited_root).expect("read").is_empty());
            let (_, fresh) = build(policy, &[]);
            assert_eq!(edited_root, fresh, "any -> empty at {cutoff}");
            assert_eq!(
                measured.payload_acquired, 0,
                "a complete deletion acquired payload bytes at {cutoff}"
            );
            assert_eq!(
                measured.acquired.len(),
                1,
                "a complete deletion acquires only the file state at {cutoff}"
            );
            assert_eq!(
                measured.exclusive_discarded.len(),
                coverage(&store, root).len(),
                "every base payload is discarded by a complete deletion"
            );
            measured.assert_charge(&store, root, &format!("to-empty-{cutoff}"));
        }

        // Control: an in-place overwrite of the same representation. The replaced
        // range *is* read - it is the no-op comparison, not a discard - but the
        // base is not read whole, and the payload bound is a few chunks.
        {
            let base = noise(large_len);
            let (store, root) = build(policy, &base);
            let at = large_len / 2;
            let replacement = noise(512);
            let mut expected = base.clone();
            expected[at..at + 512].copy_from_slice(&replacement);
            let mut replacements = Replacements::new();
            replacements.push(replacement.clone());
            let (result, edited_root, measured) = measure(
                policy,
                &store,
                root,
                vec![Edit::overwrite(at as u64, at as u64 + 512)],
                &replacements,
                expected.len() as u64,
                None,
            );
            assert_eq!(read_back(&result, edited_root).expect("read"), expected);
            // No fresh-construction root comparison here, and that is the frozen
            // contract's own rule rather than a relaxation: the registry judges a
            // *chunked* overwrite by the independent byte model
            // (`stages-3-4-verification.md` §2, `chunked-overwrite`), because a
            // localized edit deliberately keeps the retained chunk boundaries and
            // only re-scans the replacement, while a fresh construction re-chunks
            // the whole file. The whole-file results above are compared by root,
            // where the canonical bytes are a pure function of the content.
            assert!(matches!(
                representation(&result, edited_root),
                FileContent::Chunked(_)
            ));
            assert_eq!(
                measured.final_len, large_len as u64,
                "an overwrite does not change the length"
            );
            assert!(
                measured.payload_acquired <= 4 * MAX_CHUNK,
                "a 512-byte overwrite at {cutoff} acquired {} payload bytes",
                measured.payload_acquired
            );
            assert!(
                measured.payload_acquired * 2 < measured.base_payload_bytes,
                "a 512-byte overwrite at {cutoff} acquired {} of {} base bytes",
                measured.payload_acquired,
                measured.base_payload_bytes
            );
            measured.assert_charge(&store, root, &format!("in-place-{cutoff}"));
        }
    }
}

#[test]
fn a_whole_file_result_reports_no_counters_and_that_is_recorded() {
    // The accounted quantity's second instrument, stated where it is absent: the
    // whole-file assembly path publishes `EditCounters::default()`, so a
    // large -> small transition carries no `nodes_read` at all. The census above
    // is what charges that path; this case pins the gap so it cannot be mistaken
    // for a measured zero.
    let cutoff = 131_072_u64;
    let policy = policy_for(cutoff);
    let base = noise(4 * cutoff as usize);
    let (store, root) = build(policy, &base);
    let keep = cutoff / 4;
    let replacements = Replacements::new();
    let (_, _, measured) = measure(
        policy,
        &store,
        root,
        vec![Edit::delete(keep, base.len() as u64 - keep)],
        &replacements,
        2 * keep,
        Some(keep..base.len() as u64 - keep),
    );
    assert_eq!(measured.counters, EditCounters::default());
    assert!(
        measured.payload_acquired > 0,
        "the path did read the kept ranges"
    );
}
