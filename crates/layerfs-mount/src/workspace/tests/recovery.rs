#[test]
fn publication_conflict_leaves_mount_non_writable() {
    let (store, spool, directory) = paths("conflict");
    let mut winner = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x99; 32],
    )
    .unwrap();
    let mut loser = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        directory.join("loser.spool"),
        [0x9a; 32],
    )
    .unwrap();
    winner.mknod_file(ROOT_NODE, b"winner", 0o644).unwrap();
    winner.fsync().unwrap();
    loser.mknod_file(ROOT_NODE, b"loser", 0o644).unwrap();
    loser.reset_engine_counters().unwrap();
    assert!(matches!(loser.fsync(), Err(MountedError::Conflict)));
    assert_eq!(loser.lifecycle(), MountedLifecycle::Conflict);
    assert!(matches!(
        loser.mknod_file(ROOT_NODE, b"late", 0o644),
        Err(MountedError::Indeterminate)
    ));
    let counters = loser.engine_counters().unwrap();
    assert_eq!(counters.transactions_committed, 0);
    assert_eq!(counters.publication_commits, 0);
    drop(loser);
    winner.shutdown().unwrap();
    drop(winner);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn post_commit_spool_cleanup_failure_is_incomplete_not_retryable() {
    let (store, spool, directory) = paths("committed-cleanup");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x9d; 32],
    )
    .unwrap();
    mounted.reset_engine_counters().unwrap();
    let (file, handle) = mounted.create_file(ROOT_NODE, b"committed", 0o644).unwrap();
    mounted
        .write(file.node, handle, 0, b"committed-bytes")
        .unwrap();
    mounted.spool.path = directory.clone();
    assert!(matches!(
        mounted.fsync(),
        Err(MountedError::CommittedCleanup)
    ));
    assert_eq!(mounted.lifecycle(), MountedLifecycle::Incomplete);
    let counters = mounted.engine_counters().unwrap();
    assert_eq!(counters.transactions_committed, 1);
    assert_eq!(counters.publication_commits, 1);
    assert!(matches!(
        mounted.write(file.node, handle, 0, b"late"),
        Err(MountedError::Indeterminate)
    ));
    mounted.spool.path = spool.clone();
    drop(mounted);

    let mut reopened = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x9d; 32],
    )
    .unwrap();
    let file = reopened.lookup_child(ROOT_NODE, b"committed").unwrap();
    let handle = reopened.open_file(file.node, false).unwrap();
    assert_eq!(
        reopened.read(file.node, handle, 0, 32).unwrap(),
        b"committed-bytes"
    );
    reopened.release(handle).unwrap();
    reopened.shutdown().unwrap();
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn pre_commit_spool_failure_rolls_back_and_preserves_retryable_dirty_state() {
    let (store, spool, directory) = paths("precommit-spool");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x9f; 32],
    )
    .unwrap();
    let (file, handle) = mounted.create_file(ROOT_NODE, b"retry", 0o644).unwrap();
    mounted.write(file.node, handle, 0, b"retry-bytes").unwrap();
    mounted.reset_engine_counters().unwrap();
    drop(mounted.spool.file.take());
    assert!(matches!(mounted.fsync(), Err(MountedError::Corrupt)));
    assert_eq!(mounted.lifecycle(), MountedLifecycle::Live);
    let counters = mounted.engine_counters().unwrap();
    assert_eq!(counters.transactions_started, 1);
    assert_eq!(counters.transactions_committed, 0);
    assert_eq!(counters.transactions_rolled_back, 1);
    assert_eq!(counters.publication_commits, 0);
    assert!(mounted.counters().unwrap().dirty_nodes > 0);
    mounted.spool.file = Some(
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&spool)
            .unwrap(),
    );
    mounted.fsync().unwrap();
    mounted.release(handle).unwrap();
    mounted.shutdown().unwrap();
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn lost_commit_acknowledgement_reconciles_candidate_and_clears_dirty_state() {
    let (store, spool, directory) = paths("lost-ack");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0xa2; 32],
    )
    .unwrap();
    let (file, handle) = mounted
        .create_file(ROOT_NODE, b"reconciled", 0o644)
        .unwrap();
    mounted
        .write(file.node, handle, 0, b"reconciled-bytes")
        .unwrap();
    mounted.reset_engine_counters().unwrap();
    mounted.engine.inject_lost_commit_acknowledgement();
    let accepted = mounted.fsync().unwrap();
    assert_eq!(mounted.lifecycle(), MountedLifecycle::Live);
    assert_eq!(mounted.counters().unwrap().dirty_nodes, 0);
    let counters = mounted.engine_counters().unwrap();
    assert_eq!(counters.transactions_committed, 1);
    assert_eq!(counters.publication_commits, 1);
    assert!(counters.reconciliation_statements > 0);
    mounted.release(handle).unwrap();
    mounted.shutdown().unwrap();
    drop(mounted);

    let mut reopened = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0xa2; 32],
    )
    .unwrap();
    assert_eq!(reopened.accepted(), &accepted);
    let file = reopened.lookup_child(ROOT_NODE, b"reconciled").unwrap();
    let handle = reopened.open_file(file.node, false).unwrap();
    assert_eq!(
        reopened.read(file.node, handle, 0, 32).unwrap(),
        b"reconciled-bytes"
    );
    reopened.release(handle).unwrap();
    reopened.shutdown().unwrap();
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn forced_drop_discards_unacknowledged_dirty_spool_on_exact_reopen() {
    let (store, spool, directory) = paths("forced-drop");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0xa0; 32],
    )
    .unwrap();
    let (file, handle) = mounted
        .create_file(ROOT_NODE, b"unacknowledged", 0o644)
        .unwrap();
    mounted.write(file.node, handle, 0, b"discard-me").unwrap();
    assert!(spool.exists());
    drop(mounted);

    let mut reopened = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0xa0; 32],
    )
    .unwrap();
    assert!(!spool.exists());
    assert!(matches!(
        reopened.lookup_child(ROOT_NODE, b"unacknowledged"),
        Err(MountedError::NotFound)
    ));
    reopened.shutdown().unwrap();
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}
