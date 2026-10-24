//! A Save reports the chain depth of every payload object it stored and why a
//! declared base was refused: at the role's depth cap, or at the chain-work
//! budget (#352 delta outcomes).
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
fn a_chain_stops_at_the_depth_cap_and_reports_it() {
    let outcomes = chain(StoragePolicy::new(1, 131_072, 2, 4), 8 * 1024, 4);
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
    assert_eq!(&depths[..3], &[0, 1, 2]);
    // The fourth version's declared base is at the cap.
    assert!(
        outcomes[3].delta.depth_capped >= 1,
        "{:?}",
        outcomes[3].delta
    );
    assert!(depths[3] < 3);
    assert!(outcomes.iter().all(|o| o.delta.work_exceeded == 0));
    assert!(outcomes.iter().all(|o| o.delta.chunk_depths == [0; 32]));
}

#[test]
fn a_chain_stops_at_the_work_budget_before_a_deep_cap() {
    // 100 KiB versions: the 512 KiB canonical chain budget admits a chain of
    // five records, far below a depth cap of 16.
    let outcomes = chain(StoragePolicy::new(1, 131_072, 16, 4), 100 * 1024, 7);
    let depth = |o: &WriteOutcome| o.delta.whole_file_depths.iter().position(|n| *n == 1);
    let depths: Vec<_> = outcomes.iter().map(depth).collect();
    assert_eq!(depths, [0, 1, 2, 3, 4, 0, 1].map(Some));
    assert_eq!(outcomes[5].delta.work_exceeded, 1);
    assert!(outcomes.iter().all(|o| o.delta.depth_capped == 0));
    let mut total = layerfs_storage::encoding::delta::select::DeltaCounters::default();
    for o in &outcomes {
        total.accumulate(&o.delta);
    }
    assert_eq!(total.whole_file_depths[..5], [2, 2, 1, 1, 1]);
    assert_eq!(total.work_exceeded, 1);
    assert_eq!(total.prefix_selected, 5);
}
