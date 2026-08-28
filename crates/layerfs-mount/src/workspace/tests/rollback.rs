#[test]
fn retained_roots_fork_and_rollback_reopen_exact_versions() {
    let (store, spool, directory) = paths("history");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x95; 32],
    )
    .unwrap();
    let (file, handle) = mounted.create_file(ROOT_NODE, b"versioned", 0o644).unwrap();
    mounted.write(file.node, handle, 0, b"version-one").unwrap();
    let first = mounted.fsync().unwrap();
    mounted.write(file.node, handle, 0, b"version-two").unwrap();
    let second = mounted.fsync().unwrap();
    mounted.release(handle).unwrap();
    let branch = mounted.fork_ref("branch").unwrap();
    assert_eq!(branch.root, second.root);

    mounted.rollback(first.root).unwrap();
    assert_eq!(mounted.lifecycle(), MountedLifecycle::Closed);
    assert!(matches!(
        mounted.lookup_child(ROOT_NODE, b"versioned"),
        Err(MountedError::StaleHandle)
    ));
    drop(mounted);

    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x95; 32],
    )
    .unwrap();
    let file = mounted.lookup_child(ROOT_NODE, b"versioned").unwrap();
    let handle = mounted.open_file(file.node, false).unwrap();
    assert_eq!(
        mounted.read(file.node, handle, 0, 64).unwrap(),
        b"version-one"
    );
    mounted.release(handle).unwrap();

    mounted.rollback(second.root).unwrap();
    assert_eq!(mounted.lifecycle(), MountedLifecycle::Closed);
    drop(mounted);

    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x95; 32],
    )
    .unwrap();
    let file = mounted.lookup_child(ROOT_NODE, b"versioned").unwrap();
    let handle = mounted.open_file(file.node, false).unwrap();
    assert_eq!(
        mounted.read(file.node, handle, 0, 64).unwrap(),
        b"version-two"
    );
    mounted.release(handle).unwrap();
    mounted.shutdown().unwrap();
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn rollback_conflict_leaves_stale_mount_non_writable() {
    let (store, spool, directory) = paths("rollback-conflict");
    let target = {
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x95; 32],
        )
        .unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
        mounted.write(file.node, handle, 0, b"version-one").unwrap();
        let target = mounted.fsync().unwrap().root;
        mounted.write(file.node, handle, 0, b"version-two").unwrap();
        mounted.fsync().unwrap();
        mounted.release(handle).unwrap();
        mounted.shutdown().unwrap();
        target
    };
    let mut winner = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x95; 32],
    )
    .unwrap();
    let mut loser = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        directory.join("loser.spool"),
        [0x95; 32],
    )
    .unwrap();
    winner.mknod_file(ROOT_NODE, b"winner", 0o644).unwrap();
    winner.fsync().unwrap();
    assert!(matches!(
        loser.rollback(target),
        Err(MountedError::Conflict)
    ));
    assert_eq!(loser.lifecycle(), MountedLifecycle::Conflict);
    assert!(matches!(
        loser.mknod_file(ROOT_NODE, b"late", 0o644),
        Err(MountedError::Indeterminate)
    ));
    drop(loser);
    winner.shutdown().unwrap();
    drop(winner);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn rollback_ambiguity_leaves_mount_incomplete_and_non_writable() {
    let (store, spool, directory) = paths("rollback-ambiguity");
    let target = {
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x95; 32],
        )
        .unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
        mounted.write(file.node, handle, 0, b"version-one").unwrap();
        let target = mounted.fsync().unwrap().root;
        mounted.write(file.node, handle, 0, b"version-two").unwrap();
        mounted.fsync().unwrap();
        mounted.release(handle).unwrap();
        mounted.shutdown().unwrap();
        target
    };
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x95; 32],
    )
    .unwrap();
    mounted.close_store_connection().unwrap();
    assert!(matches!(
        mounted.rollback(target),
        Err(MountedError::Indeterminate)
    ));
    assert_eq!(mounted.lifecycle(), MountedLifecycle::Incomplete);
    assert!(matches!(
        mounted.mknod_file(ROOT_NODE, b"late", 0o644),
        Err(MountedError::Indeterminate)
    ));
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn shutdown_closes_mutation_admission_before_and_after_checkpoint() {
    let (store, spool, directory) = paths("closed");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x98; 32],
    )
    .unwrap();
    mounted.mknod_file(ROOT_NODE, b"accepted", 0o644).unwrap();
    let first = mounted.shutdown().unwrap();
    assert_eq!(mounted.lifecycle(), MountedLifecycle::Closed);
    assert!(matches!(
        mounted.mknod_file(ROOT_NODE, b"late", 0o644),
        Err(MountedError::StaleHandle)
    ));
    assert_eq!(mounted.shutdown().unwrap(), first);
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn paused_budget_allows_one_dirty_graceful_shutdown_checkpoint() {
    let (store, spool, directory) = paths("paused-dirty-shutdown");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x98; 32],
    )
    .unwrap();
    let (file, handle) = mounted.create_file(ROOT_NODE, b"dirty", 0o644).unwrap();
    mounted
        .write(file.node, handle, 0, b"graceful-dirty-bytes")
        .unwrap();
    mounted.release(handle).unwrap();
    let budget = mounted.byte_budget();
    budget.pause_and_wait().unwrap();
    mounted.shutdown().unwrap();
    budget.close_and_wait().unwrap();
    let counters = mounted.counters().unwrap();
    assert_eq!(counters.checkpoints, 1);
    assert_eq!(counters.operation_q_current_bytes, 0);
    assert_eq!(counters.dirty_nodes, 0);
    assert_eq!(counters.dirty_ranges, 0);
    assert_eq!(counters.spool_live_bytes, 0);
    drop(mounted);

    let mut reopened = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x98; 32],
    )
    .unwrap();
    let file = reopened.lookup_child(ROOT_NODE, b"dirty").unwrap();
    let handle = reopened.open_file(file.node, false).unwrap();
    assert_eq!(
        reopened.read(file.node, handle, 0, 64).unwrap(),
        b"graceful-dirty-bytes"
    );
    reopened.release(handle).unwrap();
    reopened.shutdown().unwrap();
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}
