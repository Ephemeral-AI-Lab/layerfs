#[test]
fn capacity_tracks_logical_spool_node_and_dirty_limits() {
    let (store, spool, directory) = paths("capacity");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x9d; 32],
    )
    .unwrap();
    let initial = mounted.capacity().unwrap();
    assert_eq!(initial.total_bytes, MAX_LOGICAL_WORKSPACE_BYTES);
    assert_eq!(initial.free_bytes, MAX_LIVE_SPOOL_BYTES);
    assert_eq!(initial.total_files, MAX_MOUNTED_NODES as u64);
    assert_eq!(initial.free_files, MAX_DIRTY_NODES as u64);

    mounted.logical_workspace_bytes = MAX_LOGICAL_WORKSPACE_BYTES;
    assert_eq!(mounted.capacity().unwrap().free_bytes, 0);
    mounted.logical_workspace_bytes = 0;
    mounted.spool.live = MAX_LIVE_SPOOL_BYTES;
    assert_eq!(mounted.capacity().unwrap().free_bytes, 0);
    mounted.spool.live = 0;

    mounted
        .dirty_nodes
        .extend((0..MAX_DIRTY_NODES).map(|index| MountedNodeId(u64::MAX - index as u64)));
    assert_eq!(mounted.capacity().unwrap().free_files, 0);
    mounted.dirty_nodes.clear();
    mounted.directory_changes = MAX_DIRECTORY_CHANGES;
    assert_eq!(mounted.capacity().unwrap().free_files, 0);
    mounted.directory_changes = 0;

    mounted.shutdown().unwrap();
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn capacity_tracks_public_dirty_checkpoint_and_cleanup_transitions() {
    let (store, spool, directory) = paths("capacity-transitions");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x9f; 32],
    )
    .unwrap();
    let initial = mounted.capacity().unwrap();

    let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
    mounted
        .write(file.node, handle, 0, &vec![0x5a; 1024 * 1024])
        .unwrap();
    mounted.release(handle).unwrap();
    let dirty = mounted.capacity().unwrap();
    assert_eq!(dirty.free_bytes, initial.free_bytes - 1024 * 1024);
    assert_eq!(dirty.free_files, initial.free_files - 2);

    mounted.fsyncdir().unwrap();
    assert_eq!(mounted.capacity().unwrap(), initial);

    mounted.unlink(ROOT_NODE, b"file").unwrap();
    mounted.fsyncdir().unwrap();
    assert_eq!(mounted.capacity().unwrap(), initial);

    mounted.shutdown().unwrap();
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn maximum_admitted_dirty_population_is_checkpointable_inside_q() {
    let (store, spool, directory) = paths("maximum-checkpoint");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x9e; 32],
    )
    .unwrap();
    for index in 0..MAX_DIRTY_NODES - 1 {
        mounted
            .mknod_file(ROOT_NODE, format!("file-{index:04}").as_bytes(), 0o644)
            .unwrap();
    }
    assert_eq!(
        mounted.counters().unwrap().dirty_nodes,
        MAX_DIRTY_NODES as u64
    );
    assert!(matches!(
        mounted.mknod_file(ROOT_NODE, b"one-too-many", 0o644),
        Err(MountedError::ResourceExhausted)
    ));
    mounted.fsync().unwrap();
    let counters = mounted.counters().unwrap();
    assert_eq!(counters.dirty_nodes, 0);
    assert_eq!(counters.operation_q_current_bytes, 0);
    assert_eq!(
        counters.operation_q_high_water_bytes,
        MAX_OPERATION_Q_BYTES as u64
    );
    mounted.shutdown().unwrap();
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}
