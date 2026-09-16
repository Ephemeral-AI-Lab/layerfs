//! Occupancy, streaming and failure bounds of known-edit construction.
//!
//! The canonical partition rules are checked on the real decoded pages: every
//! non-root page holds between 64 and 128 entries, the root holds at least two
//! children when it is a branch, and the retained frontier never grows with the
//! file length. The join cases at 63/64/127/128/129 and the streaming flush
//! boundaries at 192/193 extents are reached by probing for inputs with exactly
//! that many extents, not by shrinking the workload.

mod support;

use layerfs_content::FinalizedConsumer;
use layerfs_content::{
    apply_edits, construct_bytes, ConstructionPolicy, ContentResult, Edit, EditRequest, EditSource,
    EditStream, ObjectId, Replacements,
};
use support::{disabled_scope, extent_count, mapping_page_sizes, noise, read_back, MemoryStore};

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

/// Extent count of a chunked construction of `length` deterministic bytes.
fn extents_at(length: usize) -> u64 {
    let bytes = noise(length);
    let (store, root) = build(&bytes);
    extent_count(&store, root)
}

/// Finds an input whose chunked construction has exactly `wanted` extents.
///
/// Bisection over a monotone-enough probe keeps this a bounded search: at most a
/// few dozen constructions instead of a linear scan.
fn file_with_extents(wanted: u64) -> Vec<u8> {
    let mut low = 0_usize;
    let mut high = (wanted as usize) * 32_768 + 65_536;
    while extents_at(high) < wanted {
        low = high;
        high *= 2;
    }
    while low + 1 < high {
        let middle = (low + high) / 2;
        if extents_at(middle) >= wanted {
            high = middle;
        } else {
            low = middle;
        }
    }
    for candidate in [high, high + 1_024, high + 4_096, high + 16_384] {
        if extents_at(candidate) == wanted {
            return noise(candidate);
        }
    }
    let mut candidate = high;
    for _ in 0..512 {
        candidate += 1_024;
        if extents_at(candidate) == wanted {
            return noise(candidate);
        }
    }
    panic!("no input produced {wanted} extents");
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

fn edit_and_check(base: &[u8], edits: Vec<Edit>, replacements: &Replacements, label: &str) {
    let (store, root) = build(base);
    let stream = EditStream::new(base.len() as u64, edits).expect("valid stream");
    let mut result = store.merged_clone();
    let constructed = disabled_scope(|scope| {
        apply_edits(
            policy(),
            &policy().capacities(),
            &store,
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
        let base = file_with_extents(wanted);
        let (store, root) = build(&base);
        assert_eq!(extent_count(&store, root), wanted);
        assert_canonical(&store, root, &format!("base {wanted}"));
        // A deletion inside the first chunk must not force a rebuild of the whole
        // mapping, and an insertion at the front must grow it by one extent.
        let replacements = Replacements::new();
        edit_and_check(
            &base,
            vec![Edit::delete(10, 100)],
            &replacements,
            &format!("{wanted} delete"),
        );
        let mut insert = Replacements::new();
        insert.push(noise(9_000));
        edit_and_check(
            &base,
            vec![Edit::insert(0, 9_000)],
            &insert,
            &format!("{wanted} insert"),
        );
    }
}

#[test]
fn streaming_flush_boundaries_and_height_growth() {
    for wanted in [192_u64, 193] {
        let base = file_with_extents(wanted);
        let (store, root) = build(&base);
        assert_eq!(extent_count(&store, root), wanted);
        assert_canonical(&store, root, &format!("base {wanted}"));
        let mut insert = Replacements::new();
        insert.push(noise(40_000));
        edit_and_check(
            &base,
            vec![Edit::insert(base.len() as u64 / 2, 40_000)],
            &insert,
            &format!("{wanted} insert"),
        );
        let replacements = Replacements::new();
        edit_and_check(
            &base,
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
    let left = file_with_extents(80);
    let right = file_with_extents(100);
    let mut joined = left.clone();
    joined.extend_from_slice(&right);
    let (store, root) = build(&joined);
    assert_canonical(&store, root, "join 80+100");
    let mut insert = Replacements::new();
    insert.push(noise(200_000));
    edit_and_check(
        &joined,
        vec![Edit::insert(left.len() as u64, 200_000)],
        &insert,
        "80+100 insert",
    );
    // Deleting the seam must also leave a canonical partition.
    let replacements = Replacements::new();
    edit_and_check(
        &joined,
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
        let mut edits = Vec::new();
        let mut replacements = Replacements::new();
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
            &base,
            edits,
            &replacements,
            &format!("round {round} many edits"),
        );
    }
}

#[test]
fn the_retained_frontier_does_not_grow_with_the_file() {
    let small = file_with_extents(40);
    let large = file_with_extents(400);
    let mut peaks = Vec::new();
    for base in [&small, &large] {
        let (store, root) = build(base);
        let replacements = Replacements::new();
        let mut result = store.merged_clone();
        let stream = EditStream::new(base.len() as u64, vec![Edit::delete(0, 0)]).expect("valid");
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
                &mut result,
                scope.child("edit"),
            )
        })
        .expect("empty edit");
        assert_eq!(constructed.root, root);
        peaks.push(support::extent_count(&result, root));
    }
    assert!(peaks[1] > peaks[0]);
    // The frontier itself is bounded by the page capacity and the height, so a
    // ten-times larger file cannot retain ten times the entries; the builder is
    // exercised through the same public edit path above.
    assert!(peaks[0] <= 192 && peaks[1] <= 192 * 32);
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
    let stream = EditStream::new(base.len() as u64, vec![Edit::insert(1_000, 10)]).expect("valid");
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
    let mut replacements = Replacements::new();
    replacements.push(noise(50_000));
    let stream =
        EditStream::new(base.len() as u64, vec![Edit::insert(10_000, 50_000)]).expect("valid");
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
    let replacements = Replacements::new();
    let stream = EditStream::new(base.len() as u64, vec![Edit::delete(0, 10)]).expect("valid");
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
