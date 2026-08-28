#[test]
fn dirty_spool_compacts_inside_the_steady_physical_bound() {
    let (store, spool, directory) = paths("spool-compact");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x8e; 32],
    )
    .unwrap();
    let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
    let mut bytes = vec![0_u8; MAX_REQUEST_BYTES];
    for value in 0..70_u8 {
        bytes.fill(value);
        mounted.write(file.node, handle, 0, &bytes).unwrap();
        let counters = mounted.counters().unwrap();
        assert!(
            counters.spool_physical_bytes
                <= counters.spool_live_bytes * 2 + SPOOL_COMPACTION_SLACK_BYTES
        );
    }
    let counters = mounted.counters().unwrap();
    assert!(counters.spool_compactions >= 1);
    assert_eq!(counters.spool_live_bytes, MAX_REQUEST_BYTES as u64);
    assert_eq!(
        mounted
            .read(file.node, handle, 0, MAX_REQUEST_BYTES)
            .unwrap(),
        bytes
    );
    mounted.unlink(ROOT_NODE, b"file").unwrap();
    mounted.release(handle).unwrap();
    mounted.forget(file.node, 1);
    let counters = mounted.counters().unwrap();
    assert_eq!(counters.spool_live_bytes, 0);
    assert_eq!(counters.spool_physical_bytes, 0);
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn live_byte_decreases_normalize_spool_for_removal_and_open_orphan_checkpoint() {
    let (store, spool, directory) = paths("spool-live-decrease");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x8e; 32],
    )
    .unwrap();
    let (large, large_handle) = mounted.create_file(ROOT_NODE, b"large", 0o644).unwrap();
    let (small, small_handle) = mounted.create_file(ROOT_NODE, b"small", 0o644).unwrap();
    let chunk = vec![0x5a; MAX_REQUEST_BYTES];
    for index in 0..70_u64 {
        mounted
            .write(
                large.node,
                large_handle,
                index * MAX_REQUEST_BYTES as u64,
                &chunk,
            )
            .unwrap();
    }
    mounted.write(small.node, small_handle, 0, b"s").unwrap();
    mounted.unlink(ROOT_NODE, b"large").unwrap();
    mounted.release(large_handle).unwrap();
    mounted.forget(large.node, 1);
    let after_removal = mounted.counters().unwrap();
    assert_eq!(after_removal.spool_live_bytes, 1);
    assert!(
        after_removal.spool_physical_bytes
            <= after_removal.spool_live_bytes * 2 + SPOOL_COMPACTION_SLACK_BYTES
    );
    assert!(after_removal.spool_compactions >= 1);
    mounted.fsync().unwrap();
    mounted.release(small_handle).unwrap();

    let (orphan, orphan_handle) = mounted.create_file(ROOT_NODE, b"orphan", 0o644).unwrap();
    mounted
        .write(orphan.node, orphan_handle, 0, b"old")
        .unwrap();
    mounted.fsync().unwrap();
    mounted.unlink(ROOT_NODE, b"orphan").unwrap();
    mounted.fsync().unwrap();
    mounted.write(orphan.node, orphan_handle, 0, b"x").unwrap();
    let (checkpoint, checkpoint_handle) = mounted
        .create_file(ROOT_NODE, b"checkpoint", 0o644)
        .unwrap();
    for index in 0..70_u64 {
        mounted
            .write(
                checkpoint.node,
                checkpoint_handle,
                index * MAX_REQUEST_BYTES as u64,
                &chunk,
            )
            .unwrap();
    }
    mounted.fsync().unwrap();
    let after_checkpoint = mounted.counters().unwrap();
    assert_eq!(after_checkpoint.spool_live_bytes, 1);
    assert!(
        after_checkpoint.spool_physical_bytes
            <= after_checkpoint.spool_live_bytes * 2 + SPOOL_COMPACTION_SLACK_BYTES
    );
    assert_eq!(
        mounted.read(orphan.node, orphan_handle, 0, 8).unwrap(),
        b"xld"
    );
    mounted.release(checkpoint_handle).unwrap();
    mounted.release(orphan_handle).unwrap();
    assert_eq!(mounted.counters().unwrap().spool_physical_bytes, 0);
    mounted.shutdown().unwrap();
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn accepted_dirty_closed_unlink_drains_ranges_and_spool_after_checkpoint() {
    let (store, spool, directory) = paths("accepted-dirty-unlink");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x8e; 32],
    )
    .unwrap();
    let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
    mounted.write(file.node, handle, 0, b"accepted").unwrap();
    mounted.fsync().unwrap();
    mounted.release(handle).unwrap();
    let handle = mounted.open_file(file.node, false).unwrap();
    mounted.write(file.node, handle, 0, b"modified").unwrap();
    mounted.release(handle).unwrap();
    assert!(mounted.counters().unwrap().dirty_ranges > 0);
    assert!(mounted.counters().unwrap().spool_live_bytes > 0);
    mounted.unlink(ROOT_NODE, b"file").unwrap();
    let removed = mounted.fsync().unwrap();
    let counters = mounted.counters().unwrap();
    assert_eq!(counters.dirty_ranges, 0);
    assert_eq!(counters.spool_live_bytes, 0);
    assert_eq!(counters.spool_physical_bytes, 0);
    mounted.reset_engine_counters().unwrap();
    assert_eq!(mounted.fsync().unwrap(), removed);
    let engine = mounted.engine_counters().unwrap();
    assert_eq!(engine.transactions_started, 0);
    assert_eq!(engine.transactions_committed, 0);
    assert_eq!(engine.transactions_rolled_back, 0);
    assert_eq!(engine.publication_commits, 0);
    mounted.shutdown().unwrap();
    drop(mounted);

    let mut reopened = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x8e; 32],
    )
    .unwrap();
    assert!(matches!(
        reopened.lookup_child(ROOT_NODE, b"file"),
        Err(MountedError::NotFound)
    ));
    reopened.shutdown().unwrap();
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn dirty_closed_rename_target_drains_ranges_and_spool_after_checkpoint() {
    let (store, spool, directory) = paths("dirty-rename-target");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x8e; 32],
    )
    .unwrap();
    let (source, source_handle) = mounted.create_file(ROOT_NODE, b"source", 0o644).unwrap();
    let (target, target_handle) = mounted.create_file(ROOT_NODE, b"target", 0o644).unwrap();
    mounted
        .write(source.node, source_handle, 0, b"source-bytes")
        .unwrap();
    mounted
        .write(target.node, target_handle, 0, b"target-bytes")
        .unwrap();
    mounted.fsync().unwrap();
    mounted.release(source_handle).unwrap();
    mounted.release(target_handle).unwrap();
    let target_handle = mounted.open_file(target.node, false).unwrap();
    mounted
        .write(target.node, target_handle, 0, b"dirty-target")
        .unwrap();
    mounted.release(target_handle).unwrap();
    mounted
        .rename(ROOT_NODE, b"source", ROOT_NODE, b"target", false)
        .unwrap();
    let replaced = mounted.fsync().unwrap();
    let counters = mounted.counters().unwrap();
    assert_eq!(counters.dirty_ranges, 0);
    assert_eq!(counters.spool_live_bytes, 0);
    assert_eq!(counters.spool_physical_bytes, 0);
    mounted.reset_engine_counters().unwrap();
    assert_eq!(mounted.fsync().unwrap(), replaced);
    let engine = mounted.engine_counters().unwrap();
    assert_eq!(engine.transactions_started, 0);
    assert_eq!(engine.transactions_committed, 0);
    assert_eq!(engine.transactions_rolled_back, 0);
    assert_eq!(engine.publication_commits, 0);
    mounted.shutdown().unwrap();
    drop(mounted);

    let mut reopened = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x8e; 32],
    )
    .unwrap();
    assert!(matches!(
        reopened.lookup_child(ROOT_NODE, b"source"),
        Err(MountedError::NotFound)
    ));
    let target = reopened.lookup_child(ROOT_NODE, b"target").unwrap();
    let handle = reopened.open_file(target.node, false).unwrap();
    assert_eq!(
        reopened.read(target.node, handle, 0, 32).unwrap(),
        b"source-bytes"
    );
    reopened.release(handle).unwrap();
    reopened.shutdown().unwrap();
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}
