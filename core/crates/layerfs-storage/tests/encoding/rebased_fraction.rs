//! A re-based delta is kept only when it is at most half the object's FULL
//! record (#758 fix 4).
//!
//! #89 re-bases a candidate whose chain is at its depth cap onto the FULL root
//! of that chain. As an object drifts from that root, the delta against it
//! grows with history, and every read of the chain pays it. The rule stores a
//! new FULL instead once that delta would exceed half the FULL record, so the
//! anchor of every chain (its root plus the one delta against it) costs a read
//! at most 1.5 FULL records, and a read of a member at depth `d` at most that
//! plus its `d - 1` one-version deltas, however long the history.
//!
//! Both fixtures drift: each version replaces a fresh region of the content
//! with new incompressible bytes, so the delta against the root covers every
//! region replaced since the root was written.
use crate::metadata;
use crate::support;

use layerfs_content::{
    construct_bytes_with_predecessor, AdvisoryPredecessors, ConstructionPolicy, ContentResult,
    FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole, PredecessorBase,
};
use layerfs_storage::{Storage, StoragePolicy, WriteOutcome};
use std::sync::Arc;

/// Deterministic incompressible bytes, distinct per seed.
fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64 ^ seed.wrapping_mul(0xa076_1d64_78bd_642f);
    (0..len)
        .map(|_| {
            state ^= state << 7;
            state ^= state >> 9;
            state ^= state << 8;
            state as u8
        })
        .collect()
}

/// Version `version` of a drifting file of `len` bytes: version 0 is
/// `noise(len, 0)`, and each later version replaces the next of `regions`
/// equal regions with fresh bytes.
fn drift(bytes: &mut [u8], version: usize, regions: usize) {
    if version == 0 {
        return;
    }
    let width = bytes.len() / regions;
    let at = ((version - 1) % regions) * width;
    bytes[at..at + width].copy_from_slice(&noise(width, version as u64));
}

/// Bytes a fresh handle fetches to read `ids`.
fn fetched(metadata: &Arc<metadata::MemoryMetadata>, ids: &[ObjectId]) -> u64 {
    let storage = Storage::new(metadata.clone()).unwrap();
    storage.reader().unwrap().read_objects(ids).unwrap();
    let d = storage.diagnostics();
    d.pack_read_bytes + d.range_acquired_bytes + d.group_read_bytes
}

fn depth(outcome: &WriteOutcome) -> usize {
    let depths = &outcome.delta.whole_file_depths;
    assert_eq!(depths.iter().sum::<u64>(), 1, "{outcome:?}");
    depths.iter().position(|n| *n == 1).unwrap()
}

#[test]
fn a_drifting_file_stores_full_when_its_re_based_delta_passes_half_the_full() {
    // Whole-file depth cap 2: versions 3, 5 and 7 have a capped candidate and
    // are re-based onto the chain root. Each version replaces one eighth of a
    // 16 KiB file, so the delta of version 3 against version 0 covers three
    // eighths (kept) and that of version 5 five eighths: over half, a FULL.
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = StoragePolicy::new(1, 131_072, 2, 4);
    let storage = Storage::new(metadata.clone()).unwrap();
    let mut bytes = noise(16 * 1024, 0);
    let mut previous: Option<ObjectId> = None;
    let mut versions = Vec::new();
    let mut outcomes = Vec::new();
    for version in 0..8 {
        drift(&mut bytes, version, 8);
        let mut object = FinalizedObject::new(
            ObjectRole::WholeFile,
            support::assembled_small_object(&bytes),
        )
        .unwrap();
        if let Some(base) = previous {
            object = object.with_predecessors(AdvisoryPredecessors::explicit(base).unwrap());
        }
        previous = Some(object.id());
        let save = storage.begin_save().unwrap();
        save.accept(object.clone()).unwrap();
        outcomes.push(save.finish().unwrap());
        versions.push(object);
    }
    let depths: Vec<usize> = outcomes.iter().map(depth).collect();
    // Version 5 starts a new chain; 6 and 7 chain on it.
    assert_eq!(depths, [0, 1, 2, 1, 2, 0, 1, 2]);
    for (version, o) in outcomes.iter().enumerate() {
        let fired = u64::from(version == 5);
        assert_eq!(o.delta.rebased, u64::from(version == 3 || version == 5));
        assert_eq!(o.delta.rebased_full, fired, "{version}: {o:?}");
        assert_eq!(o.delta.full_losses, fired, "{version}: {o:?}");
        assert_eq!(o.full_records, u64::from(version == 0 || version == 5));
    }
    // Every version reads back, and its read stays within the bound: the
    // root's FULL, half a FULL for the re-based delta, and one one-version
    // delta per further level (each pack's framing counts on both sides).
    let full = fetched(&metadata, &[versions[0].id()]);
    let step = fetched(&metadata, &[versions[1].id()]) - full;
    let reader = storage.reader().unwrap();
    for (version, object) in versions.iter().enumerate() {
        assert_eq!(
            reader.read_objects(&[object.id()]).unwrap(),
            vec![object.canonical().to_vec()]
        );
        let read = fetched(&metadata, &[object.id()]);
        let inner = depths[version].saturating_sub(1) as u64 * step;
        assert!(
            2 * read <= 3 * full + 2 * inner,
            "version {version}: read {read} B, FULL {full} B, inner {inner} B"
        );
    }
}

#[test]
fn a_re_based_prefix_that_loses_anyway_is_not_counted_as_the_rule_firing() {
    // Whole-file depth cap 2: versions 1 and 2 drift one eighth each, so
    // version 3's candidate (version 2) is capped and the trial is re-based
    // onto version 0. Version 3 is fresh noise: its PREFIX against the root is
    // no shorter than its FULL record and loses the comparison on its own, so
    // the half-FULL rule changed nothing and `rebased_full` stays 0 (fix 4b;
    // fix 4 counted every re-based PREFIX over half the FULL, here 1).
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = StoragePolicy::new(1, 131_072, 2, 4);
    let storage = Storage::new(metadata.clone()).unwrap();
    let mut bytes = noise(16 * 1024, 0);
    let mut previous: Option<ObjectId> = None;
    let mut outcomes = Vec::new();
    for version in 0..4 {
        if version == 3 {
            bytes = noise(16 * 1024, 1_000);
        } else {
            drift(&mut bytes, version, 8);
        }
        let mut object = FinalizedObject::new(
            ObjectRole::WholeFile,
            support::assembled_small_object(&bytes),
        )
        .unwrap();
        if let Some(base) = previous {
            object = object.with_predecessors(AdvisoryPredecessors::explicit(base).unwrap());
        }
        previous = Some(object.id());
        let save = storage.begin_save().unwrap();
        save.accept(object).unwrap();
        outcomes.push(save.finish().unwrap());
    }
    let depths: Vec<usize> = outcomes.iter().map(depth).collect();
    assert_eq!(depths, [0, 1, 2, 0]);
    let last = &outcomes[3];
    assert_eq!(last.delta.rebased, 1, "{last:?}");
    assert_eq!(last.delta.trials, 1, "{last:?}");
    assert_eq!(last.delta.full_losses, 1, "{last:?}");
    assert_eq!(last.delta.rebased_full, 0, "{last:?}");
    let fired: u64 = outcomes.iter().map(|o| o.delta.rebased_full).sum();
    assert_eq!(fired, 0, "{outcomes:?}");
}

#[derive(Default)]
struct Emitted(Vec<FinalizedObject>);
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

/// Constructs `bytes` against `base`, saves it, and returns the file root.
fn construct(storage: &Storage, bytes: &[u8], base: Option<ObjectId>) -> (ObjectId, WriteOutcome) {
    let policy = ConstructionPolicy::frozen_default();
    let reader = storage.reader().unwrap();
    let mut emitted = Emitted::default();
    let file = layerfs_telemetry::timer::Timing::disabled("test.construct", |scope| {
        construct_bytes_with_predecessor(
            policy,
            &policy.capacities(),
            bytes,
            base.map(|root| PredecessorBase::new(&reader, root)),
            &mut emitted,
            scope.child("content"),
        )
    })
    .0
    .unwrap();
    drop(reader);
    let save = storage.begin_save().unwrap();
    for object in emitted.0 {
        save.accept(object).unwrap();
    }
    (file.root, save.finish().unwrap())
}

/// The read of a whole file from a fresh reader: its bytes and the stored
/// record bytes its program-page chains read.
fn read_file(metadata: &Arc<metadata::MemoryMetadata>, root: ObjectId) -> (Vec<u8>, u64) {
    let storage = Storage::new(metadata.clone()).unwrap();
    let reader = storage.reader().unwrap();
    let mut out = Vec::new();
    layerfs_telemetry::timer::Timing::disabled("test.read", |scope| {
        layerfs_content::read_all(&reader, root, &mut out, scope.child("read"))
    })
    .0
    .unwrap();
    (out, reader.pooled_read_counters().program_chain_encoded)
}

#[test]
fn a_drifting_extent_page_stores_whole_when_its_re_based_program_passes_half() {
    // A 1 MiB chunked file whose extent page lists every chunk; metadata depth
    // cap 2, so every second version re-bases the page onto its chain root.
    // Each version replaces one sixteenth of the file, so the program against
    // the root grows by about one sixteenth of the page per version until it
    // passes half the whole page, which is then stored whole.
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = StoragePolicy::default().with_metadata_depth(2);
    let storage = Storage::new(metadata.clone()).unwrap();
    let mut bytes = noise(1 << 20, 0);
    let mut root: Option<ObjectId> = None;
    let mut versions = Vec::new();
    let mut outcomes = Vec::new();
    for version in 0..14 {
        drift(&mut bytes, version, 16);
        let (next, outcome) = construct(&storage, &bytes, root);
        root = Some(next);
        versions.push((next, bytes.clone()));
        outcomes.push(outcome);
    }
    let fired: Vec<u64> = outcomes
        .iter()
        .map(|o| o.delta.extent.rebased_full)
        .collect();
    assert!(fired.iter().all(|n| *n <= 1), "{fired:?}");
    assert!(fired.iter().any(|n| *n == 1), "the rule fired: {fired:?}");
    let rebased: u64 = outcomes.iter().map(|o| o.delta.extent.rebased).sum();
    assert!(rebased >= 3, "{rebased}");
    let depth = |o: &WriteOutcome| o.delta.extent.depths.iter().rposition(|n| *n > 0).unwrap();
    // Every version reads back, and its page chain stays within the bound:
    // the root's whole page, half a whole page for the re-based program, and
    // one one-version program per further level.
    let (_, whole) = read_file(&metadata, versions[0].0);
    let (_, one) = read_file(&metadata, versions[1].0);
    let step = one - whole;
    for (version, ((root, expected), outcome)) in versions.iter().zip(&outcomes).enumerate() {
        let (read, encoded) = read_file(&metadata, *root);
        assert!(read == *expected, "version {version} reads back");
        let inner = depth(outcome).saturating_sub(1) as u64 * step;
        assert!(
            2 * encoded <= 3 * whole + 2 * inner,
            "version {version}: page chain {encoded} B, whole {whole} B, inner {inner} B, \
             fired {fired:?}"
        );
    }
}
