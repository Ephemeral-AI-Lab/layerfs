use super::*;

#[test]
fn full_transfer_authenticates_replays_and_releases_custody() {
    let path = test_path();
    let storage = FullStorage::create_durable(&path).unwrap();
    let owner = RequestId::from_bytes([0x41; 32]);
    let request = RequestId::from_bytes([0x42; 32]);
    let (id, canonical) = bytes_object(b"full-transfer");
    let objects = vec![(id, canonical.clone())];

    storage
        .accept_canonical_batch_pinned(owner, request, "push", &objects)
        .unwrap();
    assert!(storage.contains_authenticated_object(id).unwrap());
    assert_eq!(
        storage
            .load_canonical_authenticated_bounded(id, canonical.len())
            .unwrap(),
        canonical
    );
    assert_eq!(storage.sync_custody_rows(owner, "push").unwrap(), 2);
    storage
        .accept_canonical_batch_pinned(owner, request, "push", &objects)
        .unwrap();
    assert_eq!(storage.sync_custody_rows(owner, "push").unwrap(), 2);
    assert!(
        storage
            .accept_canonical_batch_pinned(
                RequestId::from_bytes([0x43; 32]),
                request,
                "push",
                &objects,
            )
            .is_err()
    );
    assert_eq!(storage.sync_custody_rows(owner, "push").unwrap(), 2);
    assert_eq!(storage.abort_sync_transfer(owner, "push").unwrap(), 2);
    assert_eq!(storage.sync_custody_rows(owner, "push").unwrap(), 0);
    assert!(storage.contains_authenticated_object(id).unwrap());
    drop(storage);
    fs::remove_file(path).unwrap();
}

#[test]
fn full_transfer_reaper_and_cache_authority_guard_fail_closed() {
    let path = test_path();
    let storage = FullStorage::create_durable(&path).unwrap();
    let owner = RequestId::from_bytes([0x51; 32]);
    let request = RequestId::from_bytes([0x52; 32]);
    let object = bytes_object(b"reap");
    storage
        .accept_canonical_batch_pinned(owner, request, "fetch", &[object])
        .unwrap();
    let cutoff = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 1,
    )
    .unwrap();
    assert_eq!(
        storage.reap_one_abandoned_sync(cutoff).unwrap(),
        Some((owner, "fetch".to_owned(), 2))
    );
    drop(storage);
    fs::remove_file(path).unwrap();

    let cache_path = test_path();
    let cache = FullStorage::create_cache(&cache_path, [0x53; 32]).unwrap();
    let object = bytes_object(b"cache-guard");
    assert!(cache
        .accept_canonical_batch_pinned(owner, request, "prepare", &[object])
        .is_err());
    drop(cache);
    fs::remove_file(cache_path).unwrap();
}
