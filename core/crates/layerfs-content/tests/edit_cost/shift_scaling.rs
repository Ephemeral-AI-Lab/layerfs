//! Issue #316 diagnostic: does one middle insert or delete of 4 KiB reuse the
//! untouched subtrees, and is its work independent of the file size?
//!
//! Not a product test and not timing evidence. It drives the same public entry
//! point the Workspace Commit uses (`apply_indexed_edits_view_backed` over a
//! retained `FileView`) with the shared in-memory helpers, and counts what the
//! provider was asked for and what the consumer was handed. The counts are
//! printed as `SHIFT ...` lines; run with `--nocapture`.
//!
//! The two larger sizes are ignored by default so an ordinary package run does
//! not build half a gibibyte of fixtures; select them explicitly.

#[path = "../support/mod.rs"]
mod support;

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::time::Instant;

use layerfs_content::file::mapping::{
    chunk_canonical_len, decode_file_state, decode_node_with_context, encode_chunk_object,
    ExtentNode, ExtentSlice,
};
use layerfs_content::file::{
    apply_indexed_edits_view_backed, IndexedEditRequest, IndexedEditSource,
};
use layerfs_content::{
    AuthenticatedObjects, ConstructionPolicy, ContentError, ContentResult, Edit, FileRun, FileView,
    ObjectId, ObjectRole,
};
use support::edit_runs::{Backing, Counted};
use support::{build_file, disabled_scope, edits::Edits, MemoryStore};

const MIB: usize = 1024 * 1024;
const EDIT_BYTES: u64 = 4096;

/// Deterministic incompressible bytes from a counter-based splitmix64 stream.
fn pseudo_random(len: usize, seed: u64) -> Vec<u8> {
    let mut out = vec![0_u8; len];
    let mut state = seed;
    for block in out.chunks_mut(8) {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut mixed = state;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        mixed ^= mixed >> 31;
        let bytes = mixed.to_le_bytes();
        block.copy_from_slice(&bytes[..block.len()]);
    }
    out
}

/// One replacement held in memory, counting the bytes construction asked for.
struct Inserted {
    bytes: Vec<u8>,
    served: Cell<u64>,
}

impl IndexedEditSource for Inserted {
    fn replacement_len(&self, index: usize) -> ContentResult<u64> {
        assert_eq!(index, 0, "one edit only");
        Ok(self.bytes.len() as u64)
    }

    fn read_run_at(&self, index: usize, offset: u64, output: &mut [u8]) -> ContentResult<FileRun> {
        assert_eq!(index, 0, "one edit only");
        let from = offset as usize;
        if from >= self.bytes.len() {
            return Ok(FileRun::End);
        }
        let count = output.len().min(self.bytes.len() - from);
        output[..count].copy_from_slice(&self.bytes[from..from + count]);
        self.served.set(self.served.get() + count as u64);
        Ok(FileRun::Data(count))
    }
}

/// Serves the edit's own output first and the base for everything it reused.
struct Both<'a> {
    output: &'a MemoryStore,
    base: &'a MemoryStore,
}

impl AuthenticatedObjects for Both<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter()
            .map(|id| {
                self.output
                    .canonical(*id)
                    .or_else(|| self.base.canonical(*id))
                    .map(<[u8]>::to_vec)
                    .ok_or(ContentError::MissingObject)
            })
            .collect()
    }
}

/// One mapping page with its absolute position.
struct Page {
    id: ObjectId,
    level: u8,
    origin: u64,
    bytes: u64,
}

/// Decoded mapping of one chunked file, in logical order.
struct Shape {
    level: u8,
    pages: Vec<Page>,
    extents: Vec<(u64, ExtentSlice)>,
}

fn canonical<'a>(stores: &[&'a MemoryStore], id: ObjectId) -> &'a [u8] {
    stores
        .iter()
        .find_map(|store| store.canonical(id))
        .expect("reachable object is held")
}

fn shape(stores: &[&MemoryStore], root: ObjectId) -> Shape {
    let state = decode_file_state(canonical(stores, root)).expect("chunked file state");
    let mut shape = Shape {
        level: state.tree_level,
        pages: Vec::new(),
        extents: Vec::new(),
    };
    let covered = walk(stores, state.mapping_root, true, 0, &mut shape);
    assert_eq!(covered, state.logical_len, "the walk covers the file");
    assert_eq!(shape.extents.len() as u64, state.extent_count);
    shape
}

fn walk(stores: &[&MemoryStore], id: ObjectId, root: bool, origin: u64, shape: &mut Shape) -> u64 {
    let node = decode_node_with_context(canonical(stores, id), root).expect("canonical page");
    shape.pages.push(Page {
        id,
        level: node.level(),
        origin,
        bytes: node.logical_len(),
    });
    match node {
        ExtentNode::Leaf { extents, .. } => {
            let mut at = origin;
            for extent in extents {
                shape.extents.push((at, extent));
                at += u64::from(extent.logical_length());
            }
            at - origin
        }
        ExtentNode::Branch { children, .. } => {
            let mut prior = 0_u64;
            for child in children {
                let covered = walk(stores, child.child_object_id, false, origin + prior, shape);
                assert_eq!(covered, child.cumulative_logical_end - prior);
                prior = child.cumulative_logical_end;
            }
            prior
        }
    }
}

const ROLES: [&str; 5] = ["chunk", "leaf", "branch", "file_state", "other"];

fn slot(role: ObjectRole) -> usize {
    match role {
        ObjectRole::Chunk => 0,
        ObjectRole::ExtentLeaf => 1,
        ObjectRole::ExtentBranch => 2,
        ObjectRole::FileState => 3,
        _ => 4,
    }
}

fn tally_text(tally: &[(u64, u64); 5]) -> String {
    ROLES
        .iter()
        .zip(tally.iter())
        .map(|(name, (count, bytes))| format!("{name}={count}/{bytes}B"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The base fixture shared by every edit at one size.
struct Fixture {
    bytes: Vec<u8>,
    store: MemoryStore,
    root: ObjectId,
    roles: BTreeMap<ObjectId, ObjectRole>,
    shape: Shape,
}

fn fixture(mib: usize) -> Fixture {
    let started = Instant::now();
    let bytes = pseudo_random(mib * MIB, 0x316);
    let (store, built) = build_file(&bytes);
    let roles: BTreeMap<ObjectId, ObjectRole> = store.order().iter().copied().collect();
    let shape = shape(&[&store], built.root);
    let distinct: BTreeSet<ObjectId> = shape
        .extents
        .iter()
        .map(|(_, extent)| extent.payload_object_id())
        .collect();
    assert_eq!(
        distinct.len(),
        shape.extents.len(),
        "every base chunk is distinct, so identity reuse is not an accident of repeats"
    );
    let mut by_level = BTreeMap::<u8, usize>::new();
    for page in &shape.pages {
        *by_level.entry(page.level).or_default() += 1;
    }
    let (min, max) = shape.extents.iter().fold((u32::MAX, 0), |(min, max), e| {
        (min.min(e.1.logical_length()), max.max(e.1.logical_length()))
    });
    println!(
        "SHIFT base size_mib={mib} bytes={} tree_level={} extents={} avg_chunk={} min_chunk={min} \
         max_chunk={max} pages_by_level={by_level:?} objects={} build_ms={}",
        bytes.len(),
        shape.level,
        shape.extents.len(),
        bytes.len() / shape.extents.len(),
        store.len(),
        started.elapsed().as_millis()
    );
    Fixture {
        bytes,
        store,
        root: built.root,
        roles,
        shape,
    }
}

/// Checks that a leaf chunk's identity is the public canonical chunk encoding of
/// its raw bytes, and prints the chunk covering `offset` and the one after it.
fn chunk_identity(fixture: &Fixture, mib: usize, offset: u64) {
    let index = fixture
        .shape
        .extents
        .partition_point(|(start, extent)| start + u64::from(extent.logical_length()) <= offset);
    for (start, extent) in fixture.shape.extents.iter().skip(index).take(2) {
        let length = extent.logical_length() as usize;
        let from = *start as usize;
        let raw = &fixture.bytes[from..from + length];
        let hashed = ObjectId::for_bytes(&encode_chunk_object(raw).expect("chunk encoding"));
        let stored = fixture
            .store
            .canonical(extent.payload_object_id())
            .expect("chunk bytes");
        println!(
            "SHIFT chunk size_mib={mib} covering={offset} start={start} length={length} \
             source_offset={} id={} hash_of_raw_matches={} canonical_len={}",
            extent.source_offset(),
            extent.payload_object_id(),
            hashed == extent.payload_object_id(),
            stored.len()
        );
        assert_eq!(hashed, extent.payload_object_id());
        assert_eq!(stored.len(), chunk_canonical_len(length));
    }
}

fn expected(base: &[u8], edit: Edit, inserted: &[u8], range: Range<u64>) -> Vec<u8> {
    let (start, removed, added) = (edit.start(), edit.removed_len(), edit.replacement_len());
    range
        .map(|at| {
            if at < start {
                base[at as usize]
            } else if at < start + added {
                inserted[(at - start) as usize]
            } else {
                base[(at - added + removed) as usize]
            }
        })
        .collect()
}

fn case(fixture: &Fixture, mib: usize, label: &str, edit: Edit) {
    let base_len = fixture.bytes.len() as u64;
    let source = Inserted {
        bytes: pseudo_random(edit.replacement_len() as usize, 0x0e17),
        served: Cell::new(0),
    };
    let edits = Edits::new(base_len, vec![edit]).expect("valid edit");
    let reader = Counted::new(&fixture.store);
    let view =
        disabled_scope(|scope| FileView::open(&reader, fixture.root, scope.child("retained-view")))
            .expect("view");
    let policy = ConstructionPolicy::frozen_default();
    let mut output = MemoryStore::new();
    let mut backing = Backing::default();
    let started = Instant::now();
    let result = disabled_scope(|scope| {
        apply_indexed_edits_view_backed(
            policy,
            &policy.capacities(),
            &reader,
            IndexedEditRequest {
                view: &view,
                edits: &edits,
                source: &source,
            },
            &mut output,
            &mut backing,
            scope.child("indexed-edit"),
        )
    })
    .expect("edit succeeds");
    let edit_micros = started.elapsed().as_micros();
    assert_eq!(result.logical_len, edits.final_len());

    // What the engine asked the base provider for, by role.
    let mut read = [(0_u64, 0_u64); 5];
    for id in reader.requests.borrow().iter() {
        let role = fixture.roles.get(id).copied().expect("base object");
        let bytes = fixture.store.canonical(*id).expect("base bytes").len() as u64;
        read[slot(role)].0 += 1;
        read[slot(role)].1 += bytes;
    }
    // What the engine handed the consumer, by role.
    let mut emitted = [(0_u64, 0_u64); 5];
    for (id, role) in output.order() {
        let bytes = output.canonical(*id).expect("emitted bytes").len() as u64;
        emitted[slot(*role)].0 += 1;
        emitted[slot(*role)].1 += bytes;
    }

    // Which pages of the result are base pages reused by identity.
    let after = shape(&[&output, &fixture.store], result.root);
    let base_pages: BTreeMap<ObjectId, u64> = fixture
        .shape
        .pages
        .iter()
        .map(|page| (page.id, page.origin))
        .collect();
    let base_chunks: BTreeSet<ObjectId> = fixture
        .shape
        .extents
        .iter()
        .map(|(_, extent)| extent.payload_object_id())
        .collect();
    let edit_start = edit.start();
    let edit_end = edit.start() + edit.replacement_len();
    let (mut pages_reused, mut pages_new) = (0_u64, 0_u64);
    let (mut right_leaves, mut right_reused, mut right_shifted) = (0_u64, 0_u64, 0_u64);
    let (mut left_leaves, mut left_reused) = (0_u64, 0_u64);
    let (mut right_branches, mut right_branches_reused) = (0_u64, 0_u64);
    for page in &after.pages {
        let reused = base_pages.get(&page.id).copied();
        match reused {
            Some(_) => pages_reused += 1,
            None => pages_new += 1,
        }
        let right = page.origin >= edit_end;
        let left = page.origin + page.bytes <= edit_start;
        if page.level == 0 && right {
            right_leaves += 1;
            if let Some(origin) = reused {
                right_reused += 1;
                right_shifted += u64::from(origin != page.origin);
            }
        }
        if page.level == 0 && left {
            left_leaves += 1;
            left_reused += u64::from(reused.is_some());
        }
        if page.level > 0 && right {
            right_branches += 1;
            right_branches_reused += u64::from(reused.is_some());
        }
    }
    let new_payload_extents = after
        .extents
        .iter()
        .filter(|(_, extent)| !base_chunks.contains(&extent.payload_object_id()))
        .count();
    let partial_slices = after
        .extents
        .iter()
        .filter(|(_, extent)| {
            let stored = canonical(&[&output, &fixture.store], extent.payload_object_id());
            extent.source_offset() != 0
                || chunk_canonical_len(extent.logical_length() as usize) != stored.len()
        })
        .count();

    // The result reads back as the base with the one edit applied.
    let both = Both {
        output: &output,
        base: &fixture.store,
    };
    let final_len = edits.final_len();
    let windows: Vec<Range<u64>> = if mib <= 16 {
        std::iter::once(0..final_len).collect()
    } else {
        let span = 256 * 1024;
        vec![
            0..span.min(final_len),
            edit_start.saturating_sub(span)..(edit_end + span).min(final_len),
            final_len.saturating_sub(span)..final_len,
        ]
    };
    let mut verified = 0_u64;
    for window in windows {
        let mut actual = Vec::new();
        disabled_scope(|scope| {
            layerfs_content::read_range(
                &both,
                result.root,
                window.clone(),
                &mut actual,
                scope.child("verify"),
            )
        })
        .expect("readback");
        assert!(
            actual == expected(&fixture.bytes, edit, &source.bytes, window.clone()),
            "{label} at {mib} MiB reads back wrong bytes in {window:?}"
        );
        verified += actual.len() as u64;
    }

    println!(
        "SHIFT edit size_mib={mib} op={label} at={edit_start} removed={} added={} \
         base_read[{}] emitted[{}] source_bytes_served={} \
         counters[nodes_read={} nodes_created={} payloads_created={} payload_bytes={} \
         peak_deferred_bytes={}] backing[calls={} max_batch_bytes={} records_left={}] \
         result[tree_level={} pages={} pages_reused={} pages_new={} extents={} \
         new_payload_extents={new_payload_extents} partial_slices={partial_slices}] \
         right_of_edit[leaves={right_leaves} reused={right_reused} \
         reused_at_shifted_offset={right_shifted} branches={right_branches} \
         branches_reused={right_branches_reused}] left_of_edit[leaves={left_leaves} \
         reused={left_reused}] verified_bytes={verified} edit_micros={edit_micros}",
        edit.removed_len(),
        edit.replacement_len(),
        tally_text(&read),
        tally_text(&emitted),
        source.served.get(),
        result.counters.nodes_read,
        result.counters.nodes_created,
        result.counters.payloads_created,
        result.counters.payload_bytes,
        result.counters.peak_deferred_bytes,
        backing.calls,
        backing.max_bytes,
        backing.values.len(),
        after.level,
        after.pages.len(),
        pages_reused,
        pages_new,
        after.extents.len(),
    );

    // Loose, size-independent bounds: a failure here is the finding.
    let height = u64::from(fixture.shape.level) + 1;
    assert_eq!(read[0].0, 0, "no base chunk payload is read");
    assert!(
        emitted[0].0 <= 1,
        "at most the inserted bytes become a chunk"
    );
    assert!(
        emitted[1].0 + emitted[2].0 <= 4 * height + 2,
        "emitted mapping pages follow the height, not the file"
    );
    assert!(
        read[1].0 + read[2].0 <= 4 * height + 2,
        "base mapping reads follow the height, not the file"
    );
    // The join always merges each half of the split leaf with its neighbour,
    // which is at most two rebuilt leaves on either side of the edit (observed
    // in the first run of this diagnostic); every other leaf keeps its identity.
    assert!(
        right_leaves > 0 && right_leaves - right_reused <= 2,
        "leaves to the right of the edit keep their stored identity"
    );
    assert!(
        left_leaves - left_reused <= 2,
        "leaves to the left of the edit keep their stored identity"
    );
}

fn run(mib: usize) {
    let fixture = fixture(mib);
    let middle = fixture.bytes.len() as u64 / 2;
    chunk_identity(&fixture, mib, middle);
    case(
        &fixture,
        mib,
        "insert_middle",
        Edit::insert(middle, EDIT_BYTES),
    );
    case(
        &fixture,
        mib,
        "delete_middle",
        Edit::delete(middle, middle + EDIT_BYTES),
    );
    case(&fixture, mib, "insert_head", Edit::insert(0, EDIT_BYTES));
}

#[test]
fn shift_edit_016_mib() {
    run(16);
}

#[test]
fn shift_edit_064_mib() {
    run(64);
}

#[test]
#[ignore = "large diagnostic fixture; select explicitly"]
fn shift_edit_256_mib() {
    run(256);
}

#[test]
#[ignore = "large diagnostic fixture; select explicitly"]
fn shift_edit_512_mib() {
    run(512);
}
