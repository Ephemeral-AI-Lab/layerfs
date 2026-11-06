//! A Save reports the chain depth of every payload object it stored and why a
//! declared base was not used: at the role's depth cap, or at the chain-work
//! budget, where the chain root is the base instead (#352 delta outcomes, #89).
#[path = "support/memory_metadata.rs"]
mod metadata;
mod support;

use layerfs_content::{AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::{Storage, StoragePolicy, WriteOutcome};
use std::sync::Arc;

/// Saves `versions` of one file, each in its own Save, each declaring the
/// previous version as its base; returns every Save's outcome.
fn chain(policy: StoragePolicy, len: usize, versions: usize) -> Vec<WriteOutcome> {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = policy;
    let storage = Storage::new(metadata).unwrap();
    let mut previous: Option<ObjectId> = None;
    let mut outcomes = Vec::new();
    for version in 0..versions {
        let mut raw = support::noise(len);
        raw[version * 7] ^= 0x5a;
        let mut object =
            FinalizedObject::new(ObjectRole::WholeFile, support::assembled_small_object(&raw))
                .unwrap();
        if let Some(base) = previous {
            object = object.with_predecessors(AdvisoryPredecessors::explicit(base).unwrap());
        }
        previous = Some(object.id());
        let save = storage.begin_save().unwrap();
        save.accept(object).unwrap();
        outcomes.push(save.finish().unwrap());
    }
    outcomes
}

#[test]
fn a_chain_at_the_depth_cap_re_bases_onto_its_root_and_reports_it() {
    let outcomes = chain(StoragePolicy::new(1, 131_072, 2, 4), 8 * 1024, 5);
    let depths: Vec<usize> = outcomes
        .iter()
        .map(|o| {
            assert_eq!(o.delta.whole_file_depths.iter().sum::<u64>(), 1, "{o:?}");
            o.delta
                .whole_file_depths
                .iter()
                .position(|n| *n == 1)
                .unwrap()
        })
        .collect();
    // The fourth version's declared base is at the cap: the one trial runs
    // against that chain's root instead, and the fifth chains on it (#89).
    assert_eq!(depths, [0, 1, 2, 1, 2]);
    for (index, o) in outcomes.iter().enumerate() {
        assert_eq!(
            o.delta.depth_capped,
            u64::from(index == 3),
            "{index}: {o:?}"
        );
        assert_eq!(o.delta.rebased, u64::from(index == 3), "{index}: {o:?}");
        assert_eq!(o.delta.trials, u64::from(index > 0), "{index}: {o:?}");
    }
    assert!(outcomes.iter().all(|o| o.delta.work_exceeded == 0));
    assert!(outcomes.iter().all(|o| o.delta.chunk_depths == [0; 32]));
}

#[test]
fn a_chain_re_bases_at_the_work_budget_before_a_deep_cap() {
    // 100 KiB versions: the 512 KiB canonical chain budget admits a chain of
    // five records, far below a depth cap of 16. The sixth version is a delta
    // against the chain root instead (#89), and the seventh chains on it.
    let outcomes = chain(StoragePolicy::new(1, 131_072, 16, 4), 100 * 1024, 7);
    let depth = |o: &WriteOutcome| o.delta.whole_file_depths.iter().position(|n| *n == 1);
    let depths: Vec<_> = outcomes.iter().map(depth).collect();
    assert_eq!(depths, [0, 1, 2, 3, 4, 1, 2].map(Some));
    assert_eq!(outcomes[5].delta.rebased, 1);
    assert!(outcomes.iter().all(|o| o.delta.depth_capped == 0));
    let mut total = layerfs_storage::encoding::delta::select::DeltaCounters::default();
    for o in &outcomes {
        total.accumulate(&o.delta);
    }
    assert_eq!(total.whole_file_depths[..5], [1, 2, 2, 1, 1]);
    assert_eq!(total.work_exceeded, 0);
    assert_eq!(total.rebased, 1);
    assert_eq!(total.trials, 6);
    assert_eq!(total.prefix_selected, 6);
}

#[test]
fn a_chunk_chain_at_its_depth_cap_re_bases_onto_its_root() {
    // A chunk considers only its supplied correspondence; at the chunk depth cap
    // that base's chain root is the base instead (#89), and every version reads
    // back through a fresh reader.
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = StoragePolicy::new(1, 131_072, 16, 2);
    let storage = Storage::new(metadata.clone()).unwrap();
    let mut previous: Option<ObjectId> = None;
    let mut objects = Vec::new();
    let mut depths = Vec::new();
    for version in 0..6 {
        let mut raw = support::noise(32 * 1024);
        raw[version * 7] ^= 0x5a;
        let canonical = layerfs_content::file::mapping::encode_chunk_object(&raw).unwrap();
        let mut object = FinalizedObject::new(ObjectRole::Chunk, canonical).unwrap();
        if let Some(base) = previous {
            object = object.with_predecessors(AdvisoryPredecessors::explicit(base).unwrap());
        }
        previous = Some(object.id());
        let save = storage.begin_save().unwrap();
        save.accept(object.clone()).unwrap();
        let outcome = save.finish().unwrap();
        assert_eq!(outcome.delta.whole_file_depths, [0; 32], "{outcome:?}");
        assert_eq!(outcome.delta.work_exceeded, 0, "{outcome:?}");
        let capped = u64::from(version == 3 || version == 5);
        assert_eq!(outcome.delta.depth_capped, capped, "{version}: {outcome:?}");
        assert_eq!(outcome.delta.rebased, capped, "{version}: {outcome:?}");
        depths.push(outcome.delta.chunk_depths.iter().position(|n| *n == 1));
        objects.push(object);
    }
    assert_eq!(depths, [0, 1, 2, 1, 2, 1].map(Some));
    let reader = Storage::new(metadata).unwrap();
    let reader = reader.reader().unwrap();
    for object in &objects {
        assert_eq!(
            reader.read_objects(&[object.id()]).unwrap(),
            vec![object.canonical().to_vec()]
        );
    }
}
