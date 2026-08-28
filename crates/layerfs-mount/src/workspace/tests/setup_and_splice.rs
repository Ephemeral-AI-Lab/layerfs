fn paths(label: &str) -> (PathBuf, PathBuf, PathBuf) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "layerfs-mounted-{label}-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    (
        directory.join("store.sqlite"),
        directory.join("mount.spool"),
        directory,
    )
}

#[test]
fn pending_write_overlap_unlink_open_cancels_without_publication() {
    let (store, spool, directory) = paths("cancel");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x91; 32],
    )
    .unwrap();
    mounted.reset_engine_counters().unwrap();
    let (attr, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
    mounted.write(attr.node, handle, 0, b"abcdef").unwrap();
    mounted.write(attr.node, handle, 2, b"XY").unwrap();
    assert_eq!(mounted.read(attr.node, handle, 0, 16).unwrap(), b"abXYef");
    mounted.truncate(attr.node, 4).unwrap();
    assert_eq!(mounted.read(attr.node, handle, 0, 16).unwrap(), b"abXY");
    mounted.unlink(ROOT_NODE, b"file").unwrap();
    assert_eq!(mounted.read(attr.node, handle, 0, 16).unwrap(), b"abXY");
    mounted.release(handle).unwrap();
    mounted.forget(attr.node, 1);
    let counters = mounted.counters().unwrap();
    assert_eq!(counters.pending_nodes, 0);
    assert_eq!(counters.dirty_nodes, 0);
    assert_eq!(counters.dirty_ranges, 0);
    assert_eq!(counters.spool_live_bytes, 0);
    assert_eq!(counters.spool_physical_bytes, 0);
    assert_eq!(counters.spool_appended_bytes, 8);
    assert_eq!(counters.spool_live_high_water_bytes, 6);
    assert_eq!(counters.spool_physical_high_water_bytes, 8);
    assert_eq!(counters.logical_workspace_high_water_bytes, 6);
    assert_eq!(counters.live_nodes_high_water, 2);
    assert_eq!(counters.open_handles_high_water, 1);
    assert_eq!(counters.pending_nodes_high_water, 1);
    assert_eq!(counters.dirty_nodes_high_water, 2);
    let engine = mounted.engine_counters().unwrap();
    assert_eq!(engine.transactions_started, 0);
    assert_eq!(engine.publication_commits, 0);
    assert_eq!(engine.objects_created, 0);
    mounted.shutdown().unwrap();
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn existing_non_main_ref_remounts_and_diverges_from_main() {
    let (store, spool, directory) = paths("branch-scope");
    let main_at_fork = {
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x90; 32],
        )
        .unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
        mounted.write(file.node, handle, 0, b"main-one").unwrap();
        let first = mounted.fsync().unwrap();
        let branch = mounted.fork_ref("branch").unwrap();
        assert_eq!(branch.root, first.root);
        mounted.write(file.node, handle, 0, b"main-two").unwrap();
        mounted.truncate(file.node, 8).unwrap();
        mounted.fsync().unwrap();
        mounted.release(handle).unwrap();
        mounted.shutdown().unwrap();
        first
    };
    let branch_after = {
        let mut mounted = MountedWorkspace::open(
            &store,
            "branch",
            IntegrityMode::TrustedLocalDev,
            directory.join("branch.spool"),
            [0x90; 32],
        )
        .unwrap();
        assert_eq!(mounted.accepted().root, main_at_fork.root);
        let file = mounted.lookup_child(ROOT_NODE, b"file").unwrap();
        let handle = mounted.open_file(file.node, false).unwrap();
        assert_eq!(mounted.read(file.node, handle, 0, 32).unwrap(), b"main-one");
        mounted.write(file.node, handle, 0, b"branch!!").unwrap();
        let state = mounted.fsync().unwrap();
        mounted.release(handle).unwrap();
        mounted.shutdown().unwrap();
        state
    };
    let mut main = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x90; 32],
    )
    .unwrap();
    let file = main.lookup_child(ROOT_NODE, b"file").unwrap();
    let handle = main.open_file(file.node, false).unwrap();
    assert_eq!(main.read(file.node, handle, 0, 32).unwrap(), b"main-two");
    main.release(handle).unwrap();
    main.shutdown().unwrap();
    drop(main);
    let branch = MountedWorkspace::open(
        &store,
        "branch",
        IntegrityMode::TrustedLocalDev,
        directory.join("branch-reopen.spool"),
        [0x90; 32],
    )
    .unwrap();
    assert_eq!(branch.accepted(), &branch_after);
    drop(branch);
    assert!(matches!(
        MountedWorkspace::open(
            &store,
            "missing",
            IntegrityMode::TrustedLocalDev,
            directory.join("missing.spool"),
            [0x90; 32]
        ),
        Err(MountedError::NotFound)
    ));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn closing_budget_cancels_waiters_and_drains_existing_reservations() {
    let budget = Arc::new(ByteBudget::new(4));
    let held = budget.reserve(4).unwrap();
    let waiter_budget = budget.clone();
    let waiter = std::thread::spawn(move || waiter_budget.reserve(1));
    let closer_budget = budget.clone();
    let closer = std::thread::spawn(move || closer_budget.close_and_wait());
    assert!(matches!(waiter.join().unwrap(), Err(MountedError::Busy)));
    drop(held);
    closer.join().unwrap().unwrap();
    assert_eq!(budget.observation().unwrap().0, 0);
    assert!(matches!(budget.reserve(1), Err(MountedError::Busy)));
    assert!(matches!(budget.try_reserve(1), Err(MountedError::Busy)));
}

#[test]
fn mounted_splice_reuses_direct_range_replace_and_requires_remount() {
    let (store, spool, directory) = paths("splice");
    let original = (0..256 * 1024)
        .map(|index| (index as u8).wrapping_mul(13))
        .collect::<Vec<_>>();
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x8f; 32],
    )
    .unwrap();
    let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
    mounted.write(file.node, handle, 0, &original).unwrap();
    mounted.fsync().unwrap();
    mounted.release(handle).unwrap();
    mounted.working.reset_counters().unwrap();
    let receipt = mounted
        .splice_path(
            &CanonicalPath::from_bytes(b"file").unwrap(),
            64 * 1024,
            0,
            &[0x5a; 4096],
        )
        .unwrap();
    assert!(receipt.remount_required);
    assert_eq!(mounted.lifecycle(), MountedLifecycle::Closed);
    assert_eq!(receipt.counters.rope.cdc_bytes_scanned, 4096);
    assert_eq!(receipt.counters.content_payload_bytes_read(), Some(0));
    assert!(receipt.counters.content_payload_bytes_written() <= Some(4096));
    assert_eq!(receipt.counters.namespace.nodes_created, 0);
    assert_eq!(receipt.counters.operation_q_terminal_bytes, 0);
    assert_eq!(
        receipt.counters.operation_q_high_water_bytes,
        MAX_OPERATION_Q_BYTES as u64
    );
    assert_eq!(mounted.counters().unwrap().splices, 1);
    let locality = mounted.working.counters().unwrap();
    assert_eq!(locality.candidate_full_scans, 0, "{locality:?}");
    assert!(locality.candidate_shallow_bindings >= 2, "{locality:?}");
    assert!(matches!(
        mounted.lookup_child(ROOT_NODE, b"file"),
        Err(MountedError::StaleHandle)
    ));
    drop(mounted);

    let engine = Engine::open_with_mode(&store, IntegrityMode::TrustedLocalDev).unwrap();
    let record = layerfs_core::logical::resolve(
        &engine,
        receipt.before.root,
        &CanonicalPath::from_bytes(b"file").unwrap(),
        &mut layerfs_core::logical::LogicalCounters::default(),
    )
    .unwrap()
    .record;
    let mut rope = RopeCounters::default();
    let plan = read_plan(&engine, FileStateRoot(record.content_root), &mut rope).unwrap();
    let mut old = Vec::new();
    read_range_with_plan(&engine, &plan, 0..plan.logical_len(), &mut old).unwrap();
    assert_eq!(old, original);
    drop(engine);

    let mut expected = original;
    expected.splice(64 * 1024..64 * 1024, [0x5a; 4096]);
    let mut reopened = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x8f; 32],
    )
    .unwrap();
    assert_eq!(reopened.accepted(), &receipt.after);
    let file = reopened.lookup_child(ROOT_NODE, b"file").unwrap();
    let handle = reopened.open_file(file.node, false).unwrap();
    assert_eq!(
        reopened.read(file.node, handle, 0, expected.len()).unwrap(),
        expected
    );
    reopened.release(handle).unwrap();
    reopened.shutdown().unwrap();
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn mounted_splice_conflict_and_post_visibility_uncertainty_fail_closed() {
    let (store, spool, directory) = paths("splice-fail-closed");
    let mut initial = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x8f; 32],
    )
    .unwrap();
    let (file, handle) = initial.create_file(ROOT_NODE, b"file", 0o644).unwrap();
    initial.write(file.node, handle, 0, b"original").unwrap();
    initial.fsync().unwrap();
    initial.release(handle).unwrap();
    initial.shutdown().unwrap();
    drop(initial);

    let mut winner = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x8f; 32],
    )
    .unwrap();
    let mut loser = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        directory.join("loser.spool"),
        [0x8f; 32],
    )
    .unwrap();
    winner
        .splice_path(
            &CanonicalPath::from_bytes(b"file").unwrap(),
            0,
            8,
            b"winner",
        )
        .unwrap();
    assert!(matches!(
        loser.splice_path(&CanonicalPath::from_bytes(b"file").unwrap(), 0, 8, b"loser"),
        Err(MountedError::Conflict)
    ));
    assert_eq!(loser.lifecycle(), MountedLifecycle::Conflict);
    assert!(matches!(
        loser.mknod_file(ROOT_NODE, b"late", 0o644),
        Err(MountedError::Indeterminate)
    ));
    drop(winner);
    drop(loser);

    let mut uncertain = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x8f; 32],
    )
    .unwrap();
    uncertain.splice_post_visibility_uncertainty = true;
    assert!(matches!(
        uncertain.splice_path(&CanonicalPath::from_bytes(b"file").unwrap(), 0, 6, b"final"),
        Err(MountedError::Indeterminate)
    ));
    assert_eq!(uncertain.lifecycle(), MountedLifecycle::Incomplete);
    assert!(matches!(
        uncertain.mknod_file(ROOT_NODE, b"late", 0o644),
        Err(MountedError::Indeterminate)
    ));
    drop(uncertain);

    let mut reopened = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        directory.join("reopened.spool"),
        [0x8f; 32],
    )
    .unwrap();
    let file = reopened.lookup_child(ROOT_NODE, b"file").unwrap();
    let handle = reopened.open_file(file.node, false).unwrap();
    assert_eq!(reopened.read(file.node, handle, 0, 32).unwrap(), b"final");
    reopened.release(handle).unwrap();
    reopened.shutdown().unwrap();
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}
