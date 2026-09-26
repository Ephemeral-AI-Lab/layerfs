//! Occupancy, streaming and failure bounds of known-edit construction.
//!
//! The canonical partition rules are checked on the real decoded pages: every
//! non-root page holds between 64 and 128 entries, the root holds at least two
//! children when it is a branch, and the retained frontier never grows with the
//! file length. The join cases at 63/64/127/128/129 and the streaming flush
//! boundaries at 192/193 extents use the exact deterministic inputs captured from
//! the original search. Their extent counts are still checked on every run.

mod support;

use layerfs_content::FinalizedConsumer;
use layerfs_content::{
    apply_edits, construct_bytes, ConstructionPolicy, ContentError, ContentResult, Edit,
    EditRequest, EditSource, ObjectId, ObjectRole,
};
use support::{
    disabled_scope, edits::Edits, edits::Parts, extent_count, mapping_page_sizes, noise, read_back,
    MemoryStore,
};

fn policy() -> ConstructionPolicy {
    ConstructionPolicy::frozen_default()
}

fn build(bytes: &[u8]) -> (MemoryStore, ObjectId) {
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
    .expect("construction");
    (store, constructed.root)
}

/// Exact lengths captured from the original bisection in the append-only
/// issue192 review-boundary-input-capture-20260920 receipt. Reuse the inputs,
/// not the result of the operation under test; keep the extent-count assertion.
fn file_with_extents(wanted: u64) -> (Vec<u8>, MemoryStore, ObjectId) {
    let length = match wanted {
        40 => 723_887,
        63 => 1_136_450,
        64 => 1_153_312,
        80 => 1_479_513,
        100 => 1_876_987,
        127 => 2_362_757,
        128 => 2_384_497,
        129 => 2_403_852,
        192 => 3_654_982,
        193 => 3_684_001,
        _ => panic!("no frozen input with {wanted} extents"),
    };
    let bytes = noise(length);
    let (store, root) = build(&bytes);
    assert_eq!(extent_count(&store, root), wanted);
    (bytes, store, root)
}

/// Checks the canonical partition of every page under `root`.
fn assert_canonical(store: &MemoryStore, root: ObjectId, label: &str) {
    for (level, is_root, entries) in mapping_page_sizes(store, root) {
        assert!(
            entries <= 128,
            "{label}: page at level {level} holds {entries} entries"
        );
        if !is_root {
            assert!(
                entries >= 64,
                "{label}: non-root page at level {level} holds only {entries} entries"
            );
        }
        if is_root && level > 0 {
            assert!(
                entries >= 2,
                "{label}: root branch holds only {entries} children"
            );
        }
    }
}

fn edit_and_check(
    base: (&MemoryStore, ObjectId, u64),
    edits: Vec<Edit>,
    replacements: &Parts,
    label: &str,
) {
    let (store, root, length) = base;
    let stream = Edits::new(length, edits).expect("valid stream");
    let mut result = store.merged_clone();
    let constructed = disabled_scope(|scope| {
        apply_edits(
            policy(),
            &policy().capacities(),
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
    assert_eq!(
        constructed.logical_len,
        stream.final_len(),
        "{label}: length"
    );
    assert_canonical(&result, constructed.root, label);
    let bytes = read_back(&result, constructed.root).expect("reads back");
    assert_eq!(bytes.len() as u64, stream.final_len(), "{label}: bytes");
}

#[test]
fn join_occupancy_at_every_boundary() {
    for wanted in [63_u64, 64, 127, 128, 129] {
        let (base, store, root) = file_with_extents(wanted);
        assert_eq!(extent_count(&store, root), wanted);
        assert_canonical(&store, root, &format!("base {wanted}"));
        // A deletion inside the first chunk must not force a rebuild of the whole
        // mapping, and an insertion at the front must grow it by one extent.
        let replacements = Parts::new();
        edit_and_check(
            (&store, root, base.len() as u64),
            vec![Edit::delete(10, 100)],
            &replacements,
            &format!("{wanted} delete"),
        );
        let mut insert = Parts::new();
        insert.push(noise(9_000));
        edit_and_check(
            (&store, root, base.len() as u64),
            vec![Edit::insert(0, 9_000)],
            &insert,
            &format!("{wanted} insert"),
        );
    }
}

#[test]
fn streaming_flush_boundaries_and_height_growth() {
    for wanted in [192_u64, 193] {
        let (base, store, root) = file_with_extents(wanted);
        assert_eq!(extent_count(&store, root), wanted);
        assert_canonical(&store, root, &format!("base {wanted}"));
        let mut insert = Parts::new();
        insert.push(noise(40_000));
        edit_and_check(
            (&store, root, base.len() as u64),
            vec![Edit::insert(base.len() as u64 / 2, 40_000)],
            &insert,
            &format!("{wanted} insert"),
        );
        let replacements = Parts::new();
        edit_and_check(
            (&store, root, base.len() as u64),
            vec![Edit::delete(0, base.len() as u64 - 1)],
            &replacements,
            &format!("{wanted} collapse"),
        );
    }
}

#[test]
fn a_join_of_two_full_pages_stays_canonical() {
    // 80 + 100 extents joined into one file: individually valid leaves are not
    // automatically final, so the joined mapping must still satisfy the canonical
    // partition after the edit.
    let (left, _, _) = file_with_extents(80);
    let (right, _, _) = file_with_extents(100);
    let mut joined = left.clone();
    joined.extend_from_slice(&right);
    let (store, root) = build(&joined);
    assert_canonical(&store, root, "join 80+100");
    let mut insert = Parts::new();
    insert.push(noise(200_000));
    edit_and_check(
        (&store, root, joined.len() as u64),
        vec![Edit::insert(left.len() as u64, 200_000)],
        &insert,
        "80+100 insert",
    );
    // Deleting the seam must also leave a canonical partition.
    let replacements = Parts::new();
    edit_and_check(
        (&store, root, joined.len() as u64),
        vec![Edit::delete(
            left.len() as u64 - 1_000,
            left.len() as u64 + 1_000,
        )],
        &replacements,
        "80+100 seam delete",
    );
}

#[test]
fn many_files_and_many_edits_stay_bounded() {
    for round in 0..8_u64 {
        let base = noise(150_000 + round as usize * 1_000);
        let (store, root) = build(&base);
        let mut edits = Vec::new();
        let mut replacements = Parts::new();
        let mut position = 100_u64;
        for index in 0..64_u64 {
            if index % 4 == 3 {
                edits.push(Edit::delete(position, position + 200));
                replacements.push(Vec::new());
            } else {
                edits.push(Edit::overwrite(position, position + 300));
                replacements.push(noise(300));
            }
            position += 700 + index;
        }
        edit_and_check(
            (&store, root, base.len() as u64),
            edits,
            &replacements,
            &format!("round {round} many edits"),
        );
    }
}

/// The retained frontier is a function of the tree the operation ends up with,
/// not of the file length and not of how many edits produced it.
///
/// Each case applies a growing number of edits to one fixed-shape chunked file
/// through the real public edit path and reads the counters that path reports:
/// every edit must have created its payload and changed the mapping, the emitted
/// mapping objects must equal `nodes_created`, and the peak frontier must stay at
/// the bound the final tree and one replacement scan need - a superseded draft is
/// released, so the peak cannot grow with the edit count.
#[test]
fn the_retained_frontier_does_not_grow_with_the_edit_count() {
    let (base, base_store, root) = file_with_extents(40);
    let base_len = base.len() as u64;
    let base_extents = extent_count(&base_store, root);
    let mut peaks = Vec::new();
    for edits in [1_u64, 2, 4, 8, 16] {
        let mut replacements = Parts::new();
        let mut declared = Vec::new();
        let mut position = 1_000_u64;
        for index in 0..edits {
            let length = 300 + index;
            declared.push(Edit::overwrite(position, position + length));
            replacements.push(noise(length as usize));
            position += 2_048;
        }
        let stream = Edits::new(base_len, declared).expect("valid stream");
        let mut tracking = support::TrackingConsumer::new();
        let constructed = disabled_scope(|scope| {
            apply_edits(
                policy(),
                &policy().capacities(),
                &base_store,
                EditRequest {
                    root,
                    edits: &stream,
                    source: &replacements,
                },
                &mut tracking,
                scope.child("edit"),
            )
        })
        .expect("chunked edit");
        assert_eq!(
            constructed.logical_len, base_len,
            "overwrites keep the length"
        );
        assert_eq!(
            constructed.counters.payloads_created, edits,
            "every edit created exactly one replacement payload"
        );
        let mapping = tracking
            .store
            .roles()
            .iter()
            .filter(|role| matches!(role, ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch))
            .count() as u64;
        assert!(mapping >= 1, "the edit published no mapping page");
        assert_eq!(
            constructed.counters.nodes_created, mapping,
            "nodes_created must equal the emitted mapping objects"
        );
        tracking.assert_children_precede_parents();
        let mut merged = base_store.merged_clone();
        merged.absorb(&tracking.store);
        assert!(
            extent_count(&merged, constructed.root) > base_extents,
            "the mapping did not change, so the frontier was never exercised"
        );
        peaks.push(constructed.counters.peak_deferred_bytes);
    }
    // One boundary path, one replacement scan and the final page fit far inside
    // this bound; a frontier that merely accumulated drafts would exceed it many
    // times over at 16 edits.
    assert!(
        peaks.iter().all(|peak| *peak <= 64 * 1_024),
        "the frontier grew past the bound the tree needs: {peaks:?}"
    );
    // Doubling the edit count must not double the frontier. One replacement scan
    // plus the two extents each overwrite adds to the final page fit inside a
    // kilobyte; a frontier that accumulated superseded drafts instead grows
    // linearly, and this case measured 6 308 -> 129 728 bytes over the same steps
    // when the release rule was removed.
    for window in peaks.windows(2) {
        assert!(
            window[1] <= window[0] + 1_024,
            "doubling the edit count doubled the frontier: {peaks:?}"
        );
    }
    // The passing vector is printed rather than only described: it is the with-fix
    // half of the control pair, and without this line it exists only as prose in
    // the packet README, which is what the 2026-09-17 review recorded as F-19(b).
    println!("MEASURED frontier peaks (bytes), 1/2/4/8/16 edits: {peaks:?}");
}

#[test]
fn a_source_that_ends_early_fails_without_emitting_a_root() {
    struct ShortSource {
        bytes: Vec<u8>,
    }
    impl EditSource for ShortSource {
        fn replacement_len(&self, _index: usize) -> u64 {
            self.bytes.len() as u64
        }
        fn read_at(&self, _index: usize, offset: u64, buffer: &mut [u8]) -> ContentResult<usize> {
            // Declares ten bytes but reaches end of input after the first.
            if offset >= 1 {
                return Ok(0);
            }
            buffer[0] = self.bytes[0];
            Ok(1)
        }
    }
    let base = noise(600_000);
    let (store, root) = build(&base);
    let source = ShortSource { bytes: noise(10) };
    let stream = Edits::new(base.len() as u64, vec![Edit::insert(1_000, 10)]).expect("valid");
    let error = disabled_scope(|scope| {
        let mut result = MemoryStore::new();
        apply_edits(
            policy(),
            &policy().capacities(),
            &store,
            EditRequest {
                root,
                edits: &stream,
                source: &source,
            },
            &mut result,
            scope.child("edit"),
        )
    })
    .unwrap_err();
    assert!(
        matches!(error, layerfs_content::ContentError::Io),
        "got {error}"
    );
}

#[test]
fn a_consumer_that_rejects_stops_the_edit_once() {
    struct Rejecting {
        accepted: usize,
        limit: usize,
    }
    impl layerfs_content::FinalizedConsumer for Rejecting {
        fn accept(&mut self, _object: layerfs_content::FinalizedObject) -> ContentResult<()> {
            if self.accepted >= self.limit {
                return Err(layerfs_content::ContentError::OutputRejected);
            }
            self.accepted += 1;
            Ok(())
        }
    }
    let base = noise(400_000);
    let (store, root) = build(&base);
    let mut replacements = Parts::new();
    replacements.push(noise(50_000));
    let stream = Edits::new(base.len() as u64, vec![Edit::insert(10_000, 50_000)]).expect("valid");
    let mut consumer = Rejecting {
        accepted: 0,
        limit: 1,
    };
    let error = disabled_scope(|scope| {
        apply_edits(
            policy(),
            &policy().capacities(),
            &store,
            EditRequest {
                root,
                edits: &stream,
                source: &replacements,
            },
            &mut consumer,
            scope.child("edit"),
        )
    })
    .unwrap_err();
    assert_eq!(error, layerfs_content::ContentError::OutputRejected);
    assert_eq!(consumer.accepted, 1, "the rejection is returned once");
}

#[test]
fn a_missing_base_root_is_reported_as_missing() {
    let base = noise(300_000);
    let (store, root) = build(&base);
    let mut stripped = MemoryStore::new();
    for (id, role) in store.order() {
        if *id == root {
            continue;
        }
        let object = layerfs_content::FinalizedObject::new(
            *role,
            store.canonical(*id).expect("bytes").to_vec(),
        )
        .expect("canonical");
        stripped.accept(object).expect("copy");
    }
    let replacements = Parts::new();
    let stream = Edits::new(base.len() as u64, vec![Edit::delete(0, 10)]).expect("valid");
    let error = disabled_scope(|scope| {
        let mut result = MemoryStore::new();
        apply_edits(
            policy(),
            &policy().capacities(),
            &stripped,
            EditRequest {
                root,
                edits: &stream,
                source: &replacements,
            },
            &mut result,
            scope.child("edit"),
        )
    })
    .unwrap_err();
    assert_eq!(error, layerfs_content::ContentError::MissingObject);
}

/// A sink whose declared capacity binds during emission fails the edit once.
///
/// The reviewed coverage had a consumer that rejects on a *count*; this case
/// bounds a sink by the bytes it will hold, fills it in the middle of the
/// children-first emission a chunked multi-edit produces, and requires: the
/// declared failure is returned once, no root is claimed, the accepted prefix is
/// still children-first, and the identical edit succeeds against an unbounded
/// sink - so the failure is the sink's capacity and not the edit.
#[test]
fn a_bounded_sink_that_fills_during_emission_fails_the_edit_once() {
    struct Bounded {
        budget: usize,
        held: usize,
        seen: Vec<ObjectId>,
    }
    impl FinalizedConsumer for Bounded {
        fn accept(&mut self, object: layerfs_content::FinalizedObject) -> ContentResult<()> {
            let length = object.canonical_len();
            if self.held + length > self.budget {
                return Err(ContentError::OutputRejected);
            }
            self.held += length;
            self.seen.push(object.id());
            Ok(())
        }
    }

    let base = noise(400_000);
    let (store, root) = build(&base);
    let mut replacements = Parts::new();
    let mut edits = Vec::new();
    let mut position = 1_000_u64;
    for index in 0..8_u64 {
        let length = 300 + index;
        edits.push(Edit::overwrite(position, position + length));
        replacements.push(noise(length as usize));
        position += 4_096;
    }
    let stream = Edits::new(base.len() as u64, edits).expect("valid stream");

    // The same edit against an unbounded store, for the totals to compare with.
    let mut complete = MemoryStore::new();
    disabled_scope(|scope| {
        apply_edits(
            policy(),
            &policy().capacities(),
            &store,
            EditRequest {
                root,
                edits: &stream,
                source: &replacements,
            },
            &mut complete,
            scope.child("edit"),
        )
    })
    .expect("unbounded edit");
    let total = complete.order().len();
    assert!(total > 2, "the edit emits several objects: {total}");

    let mut sink = Bounded {
        budget: complete.canonical_bytes() as usize / 2,
        held: 0,
        seen: Vec::new(),
    };
    let error = disabled_scope(|scope| {
        apply_edits(
            policy(),
            &policy().capacities(),
            &store,
            EditRequest {
                root,
                edits: &stream,
                source: &replacements,
            },
            &mut sink,
            scope.child("edit"),
        )
    })
    .expect_err("the bounded sink fills before the edit finishes");
    assert_eq!(error, ContentError::OutputRejected);
    assert!(
        !sink.seen.is_empty() && sink.seen.len() < total,
        "the capacity bound must bind mid-emission: {} of {total}",
        sink.seen.len()
    );

    // The accepted prefix is still children-first.
    let mut position_of = std::collections::BTreeMap::new();
    for (index, id) in sink.seen.iter().enumerate() {
        position_of.entry(*id).or_insert(index);
    }
    for (index, id) in sink.seen.iter().enumerate() {
        let role = complete
            .order()
            .iter()
            .find(|(candidate, _)| candidate == id)
            .map(|(_, role)| *role)
            .expect("the accepted object is one the edit emits");
        let canonical = complete.canonical(*id).expect("canonical bytes");
        for reference in support::references_of(canonical, role) {
            if let Some(child) = position_of.get(&reference) {
                assert!(*child < index, "{role:?} {id} preceded its child");
            }
        }
    }
}

/// A 1,024-run canonical construction with bounded retained state.
#[test]
fn a_thousand_separated_edits_keep_bounded_state() {
    const RUNS: usize = 1_024;

    // Well separated, non-overlapping four-byte overwrites: 1,024 of them span
    // fewer than 33 000 bytes of a 400 000-byte base.
    let declared = |count: usize| {
        (0..count)
            .map(|index| {
                let start = 1_000 + index as u64 * 8;
                Edit::overwrite(start, start + 4)
            })
            .collect::<Vec<_>>()
    };
    for count in [RUNS - 1, RUNS] {
        let stream = Edits::new(400_000, declared(count))
            .unwrap_or_else(|error| panic!("{count} edits are accepted: {error}"));
        assert_eq!(stream.edits().len(), count);
    }
    // Every edit is applied, the
    // frontier stays bounded and the result is exactly the expected bytes.
    let base = noise(400_000);
    let (store, root) = build(&base);
    let edits = declared(RUNS);
    let mut replacements = Parts::new();
    let mut expected = base.clone();
    for (index, edit) in edits.iter().enumerate() {
        let mut replacement = noise(4);
        replacement[0] = (index & 0xff) as u8;
        replacements.push(replacement.clone());
        expected.splice(
            edit.start() as usize..edit.end() as usize,
            replacement.iter().copied(),
        );
    }
    let stream = Edits::new(base.len() as u64, edits).expect("accepted stream");
    // The consumer already holds the base objects, so payloads the delta lane reuses
    // and payloads it writes are both readable from it.
    let mut consumer = store.merged_clone();
    let constructed = disabled_scope(|scope| {
        apply_edits(
            policy(),
            &policy().capacities(),
            &store,
            EditRequest {
                root,
                edits: &stream,
                source: &replacements,
            },
            &mut consumer,
            scope.child("edit"),
        )
    })
    .expect("ceiling-sized edit");
    assert_eq!(constructed.logical_len, expected.len() as u64);
    println!("MEASURED ceiling-run counters: {:?}", constructed.counters);
    // Every edit landed as its own payload, so the run really did cross the whole
    // stream, and the retained frontier stayed page-scale rather than growing with
    // the edit count.
    assert_eq!(
        constructed.counters.payloads_created, RUNS as u64,
        "one payload per edit: {:?}",
        constructed.counters
    );
    assert!(
        constructed.counters.peak_deferred_bytes < 512 * 1_024,
        "the frontier stayed page-scale at the edit ceiling: {:?}",
        constructed.counters
    );
    let observed = read_back(&consumer, constructed.root).expect("ceiling result reads back");
    assert_eq!(
        observed, expected,
        "the ceiling run produced the expected bytes"
    );
}

/// The frontier does not grow with **repeated in-place** edits either.
///
/// The first frontier case spreads its edits across the file, so the mapping stays
/// a single page. This case rewrites the same small region over and over, which
/// grows one page past the entry capacity and turns the mapping into a branch: it
/// is the shape in which a release rule that only follows the simple boundary
/// paths leaks a branch per edit. The peak must stay at the tree the file ends up
/// with, and the result must still be the expected bytes.
#[test]
fn the_frontier_does_not_grow_with_repeated_in_place_edits() {
    // Edits confined to one small region, one round after another, so the mapping
    // turns into a branch early and every later round rewrites a page it must also
    // split and rejoin: the shape in which a release rule that follows only the
    // simple boundary paths leaks a page per edit. Four rounds of increasing size
    // share the region, so the frontier is compared with the tree each round ends
    // up with rather than with an edit count.
    let base = noise(400_000);
    let (store, root) = build(&base);
    let mut expected = base.clone();
    let mut peaks = Vec::new();
    let mut extents = Vec::new();
    for count in [128_usize, 256, 512, 1_024] {
        let first = peaks.len() * 1_024;
        let mut replacements = Parts::new();
        let edits = (0..count)
            .map(|index| {
                let start = 1_000 + ((first + index) as u64) * 8;
                Edit::overwrite(start, start + 4)
            })
            .collect::<Vec<_>>();
        for index in 0..count {
            let mut replacement = noise(4);
            replacement[0] = ((first + index) & 0xff) as u8;
            replacements.push(replacement.clone());
            let start = 1_000 + ((first + index) as u64) * 8;
            expected.splice(start as usize..start as usize + 4, replacement);
        }
        let stream = Edits::new(base.len() as u64, edits).expect("valid stream");
        let mut consumer = MemoryStore::new();
        let constructed = disabled_scope(|scope| {
            apply_edits(
                policy(),
                &policy().capacities(),
                &store,
                EditRequest {
                    root,
                    edits: &stream,
                    source: &replacements,
                },
                &mut consumer,
                scope.child("edit"),
            )
        })
        .expect("in-place edit stream");
        assert_eq!(constructed.logical_len, expected.len() as u64);
        assert_eq!(
            constructed.counters.payloads_created, count as u64,
            "every edit created a payload"
        );
        peaks.push(constructed.counters.peak_deferred_bytes);
        // The result's own extent count: the frontier is what the tree the
        // operation ends up with needs, so the two are compared directly.
        let mut merged = store.merged_clone();
        merged.absorb(&consumer);
        extents.push(support::extent_count(&merged, constructed.root) as usize);
    }
    for (index, count) in extents.iter().enumerate() {
        assert!(
            *count > 0,
            "the result has extents for {} edits",
            [128, 256, 512, 1_024][index]
        );
        assert!(
            peaks[index] <= 128 * count + 8 * 1_024,
            "the frontier exceeds the tree it ends up with: {} bytes for {count} extents",
            peaks[index]
        );
    }
    // And a thousand in-place edits leave a page-scale frontier, nowhere near the
    // 8 MiB ceiling the operation may hold.
    assert!(
        peaks[peaks.len() - 1] < 512 * 1_024,
        "the frontier is not page-scale: {peaks:?}"
    );
}

/// The deferred-state ceiling is proved out of reach, not induced.
///
/// The charge is live, so crossing `EDIT_DEFERRED_LIMIT` needs ~1 000 drafts alive
/// at once, and a draft charges at most one mapping node plus the fixed overhead.
/// A non-root page holds at least [`MIN_ENTRIES`] entries, so a frontier of P pages
/// spans at least `64(P - 1)` extents, and an edit operation can only create
/// `length / MINIMUM_CHUNK_BYTES + 3 * edits` extents (three boundary pieces per
/// edit at most, one of them a sub-minimum replacement tail). Reachability
/// therefore costs a base of roughly
/// `(64 * EDIT_DEFERRED_LIMIT / (MAX_NODE_OBJECT_BYTES + 128) - 3 * 4 096) * 8 192`
/// bytes - some hundreds of MiB - which is far outside this packet's fixture
/// budget, so the refusal in `tree.rs` is recorded as unrun and this case instead
/// checks the premise the derivation rests on: on the largest in-budget shape the
/// measured charge per live draft stays within the maximum a draft may charge.
#[test]
fn the_deferred_ceiling_charge_per_draft_stays_inside_its_derived_bound() {
    use layerfs_content::file::cdc::MINIMUM_CHUNK_BYTES;
    use layerfs_content::file::mapping::{MAX_NODE_OBJECT_BYTES, MIN_ENTRIES};
    use layerfs_content::file::EDIT_DEFERRED_LIMIT;
    const RUNS: usize = 4_096;

    // Eight MiB of base with the full edit budget spread evenly across it: the
    // largest shape a bounded fixture can offer.
    let base = noise(8 * 1024 * 1024);
    let (store, root) = build(&base);
    let edits = (0..RUNS)
        .map(|index| {
            let start = 1_000 + index as u64 * 2_048;
            Edit::overwrite(start, start + 4)
        })
        .collect::<Vec<_>>();
    let mut replacements = Parts::new();
    for index in 0..RUNS {
        let mut replacement = noise(4);
        replacement[0] = (index & 0xff) as u8;
        replacements.push(replacement);
    }
    let stream = Edits::new(base.len() as u64, edits).expect("valid stream");
    let mut consumer = store.merged_clone();
    let constructed = disabled_scope(|scope| {
        apply_edits(
            policy(),
            &policy().capacities(),
            &store,
            EditRequest {
                root,
                edits: &stream,
                source: &replacements,
            },
            &mut consumer,
            scope.child("edit"),
        )
    })
    .expect("bounded-frontier edit");
    let counters = constructed.counters;
    println!("MEASURED deferred shape: {counters:?}");
    assert_eq!(counters.payloads_created, RUNS as u64, "every edit landed");
    assert!(
        counters.peak_deferred_bytes < EDIT_DEFERRED_LIMIT,
        "the ceiling held without refusing: {counters:?}"
    );
    assert!(counters.nodes_created > 0, "a frontier was built");
    let per_draft = counters.peak_deferred_bytes / counters.nodes_created as usize;
    assert!(
        per_draft <= MAX_NODE_OBJECT_BYTES + 128,
        "a live draft charged more than one node plus overhead: {per_draft} bytes"
    );

    // The derivation, from the measured charge bound rather than from a number
    // chosen here: pages the ceiling needs, extents those pages span at the
    // occupancy floor, and the base bytes those extents imply at the chunk minimum
    // once the whole edit budget has been spent on boundary pieces.
    let drafts_needed = EDIT_DEFERRED_LIMIT.div_ceil(MAX_NODE_OBJECT_BYTES + 128);
    let extents_needed = MIN_ENTRIES * (drafts_needed - 1);
    let boundary_extents = 3 * RUNS;
    let base_floor = extents_needed.saturating_sub(boundary_extents) * MINIMUM_CHUNK_BYTES;
    println!(
        "DERIVED ceiling floor: {per_draft} B/draft measured (bound {}), \
         {drafts_needed} live pages, {extents_needed} extents, {base_floor} base bytes",
        MAX_NODE_OBJECT_BYTES + 128
    );
    assert!(
        base_floor > 256 * 1024 * 1024,
        "the deferred ceiling is out of reach for a bounded fixture: {base_floor} bytes"
    );
}
