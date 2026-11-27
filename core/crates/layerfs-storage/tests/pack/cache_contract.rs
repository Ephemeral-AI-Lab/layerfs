//! Public cache behavior: capacity release, cohort protection and owner lifetime.
use layerfs_storage::{encoding::PackCache, policy};

#[test]
fn pressure_evicts_only_cold_bodies_and_keeps_recent_siblings() {
    let mut cache = PackCache::new();
    for id in 1..=8 {
        cache
            .insert(id, vec![id as u8; policy::PACK_LIMIT])
            .unwrap();
    }
    assert_eq!(cache.retained_bytes(), policy::DEPENDENCY_PACK_CACHE_BYTES);
    assert_eq!(cache.get(&1).unwrap()[0], 1);
    cache.insert(9, vec![9; policy::PACK_LIMIT]).unwrap();
    assert!(cache.contains_key(&1));
    assert!(!cache.contains_key(&2));
    for id in 3..=9 {
        assert!(cache.contains_key(&id));
    }
    assert_eq!(cache.work().evictions, 1);
    assert_eq!(cache.work().evicted_bytes, policy::PACK_LIMIT as u64);
    assert_eq!(cache.retained_bytes(), policy::DEPENDENCY_PACK_CACHE_BYTES);
}

#[test]
fn cohort_reservation_keeps_requested_hits_while_releasing_misses_space() {
    let mut cache = PackCache::new();
    for id in 1..=8 {
        cache.insert(id, vec![0; policy::PACK_LIMIT]).unwrap();
    }
    // Oldest body is explicitly required alongside two misses.
    cache
        .reserve(&[
            (1, policy::PACK_LIMIT),
            (9, policy::PACK_LIMIT),
            (10, policy::PACK_LIMIT),
        ])
        .unwrap();
    assert!(cache.contains_key(&1));
    assert_eq!(cache.work().evictions, 2);
    cache.insert(9, vec![9; policy::PACK_LIMIT]).unwrap();
    cache.insert(10, vec![10; policy::PACK_LIMIT]).unwrap();
    assert_eq!(cache.work().evictions, 2);
    assert!(cache.contains_key(&1));
    assert_eq!(cache.retained_bytes(), policy::DEPENDENCY_PACK_CACHE_BYTES);
    assert!(cache.reserve(&[(1, policy::PACK_LIMIT + 1)]).is_err());
    assert!(cache
        .reserve(&[(1, policy::PACK_LIMIT), (1, policy::PACK_LIMIT)])
        .is_err());
}

#[test]
fn singleton_is_isolated_and_new_owner_has_no_retained_input() {
    let mut cache = PackCache::new();
    cache.insert(1, vec![0; policy::PACK_LIMIT]).unwrap();
    let length = policy::DEPENDENCY_PACK_CACHE_BYTES + 1;
    cache.reserve(&[(2, length)]).unwrap();
    assert_eq!(cache.retained_bytes(), 0);
    cache.insert(2, vec![2; length]).unwrap();
    assert!(!cache.contains_key(&1));
    cache.insert(3, vec![3; policy::PACK_LIMIT]).unwrap();
    assert!(!cache.contains_key(&2));
    assert_eq!(cache.retained_bytes(), policy::PACK_LIMIT);
    assert!(cache
        .insert(4, vec![0; policy::SINGLETON_PACK_LIMIT + 1])
        .is_err());
    cache.clear();
    assert_eq!(cache.retained_bytes(), 0);
    assert!(!PackCache::new().contains_key(&3));
}
