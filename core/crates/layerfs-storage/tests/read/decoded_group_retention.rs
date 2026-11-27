//! Decoded dependency groups retain recently used input within the same bound.
use layerfs_storage::{encoding::GroupCache, policy};
#[test]
fn overflow_keeps_recent_dependency_groups_and_evicts_only_a_cold_group() {
    let mut cache = GroupCache::new();
    let fit = policy::DECODED_GROUP_CACHE_BYTES / policy::GROUP_LIMIT;
    for id in 1..=fit {
        cache
            .insert(id as i64, 0, vec![id as u8; policy::GROUP_LIMIT])
            .unwrap();
    }
    assert_eq!(cache.retained_bytes(), policy::DECODED_GROUP_CACHE_BYTES);
    assert_eq!(cache.get(1, 0).unwrap()[0], 1);
    cache
        .insert((fit + 1) as i64, 0, vec![9; policy::GROUP_LIMIT])
        .unwrap();
    assert_eq!(cache.get(1, 0).unwrap()[0], 1);
    assert!(cache.get(2, 0).is_none());
    for id in 3..=fit + 1 {
        assert!(cache.get(id as i64, 0).is_some());
    }
    assert_eq!(cache.retained_bytes(), policy::DECODED_GROUP_CACHE_BYTES);
}

#[test]
fn duplicate_input_is_idempotent_and_invalid_admission_preserves_retention() {
    let mut cache = GroupCache::new();
    cache.insert(1, 0, vec![7; 32]).unwrap();
    cache.insert(1, 0, vec![7; 32]).unwrap();
    assert_eq!(cache.retained_bytes(), 32);
    assert!(cache.insert(1, 0, vec![8; 32]).is_err());
    assert!(cache.insert(2, 0, vec![]).is_err());
    assert!(cache.insert(0, 0, vec![1]).is_err());
    assert!(cache
        .insert(2, 0, vec![1; policy::DECODED_GROUP_CACHE_BYTES + 1])
        .is_err());
    assert_eq!(cache.retained_bytes(), 32);
    assert_eq!(cache.get(1, 0).unwrap(), &[7; 32]);
}
