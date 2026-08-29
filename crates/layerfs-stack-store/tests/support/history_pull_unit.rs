use super::*;
use layerfs_content::ObjectId;
use layerfs_storage::{CommitId, CommitRecord};

#[test]
fn deferred_facts_stay_in_memory_then_spill_after_eight_mib() {
    let root = ObjectId::for_bytes(b"fact-spool");
    let fact = Fact::Commit(CommitRecord {
        id: CommitId::derive(root, None, None),
        root_id: root,
        parent_id: None,
        merge_parent_id: None,
    });
    let mut small = DeferredFactStore::new().unwrap();
    small.stage(fact).unwrap();
    assert!(!small.spilled());
    let mut count = 0;
    small
        .visit_batches(&mut |facts| {
            count += facts.len();
            Ok(())
        })
        .unwrap();
    assert_eq!(count, 1);

    let mut large = DeferredFactStore::new().unwrap();
    for _ in 0..=(DEFERRED_MEMORY_BYTES / (fact.signing_bytes().len() + 4)) {
        large.stage(fact).unwrap();
    }
    assert!(large.spilled());
}
