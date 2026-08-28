#[test]
fn resource_limit_failures_leave_namespace_content_and_spool_atomic() {
    let (store, spool, directory) = paths("resource-preflight");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x9b; 32],
    )
    .unwrap();
    let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
    mounted.write(file.node, handle, 0, b"keep").unwrap();
    mounted.fsync().unwrap();
    mounted.release(handle).unwrap();

    assert!(matches!(
        mounted.truncate(file.node, MAX_LOGICAL_FILE_BYTES + 1),
        Err(MountedError::NoSpace)
    ));
    assert_eq!(mounted.getattr(file.node).unwrap().size, 4);
    assert_eq!(mounted.counters().unwrap().dirty_nodes, 0);

    let next_handle = mounted.next_handle;
    mounted.next_handle = u64::MAX;
    assert!(matches!(
        mounted.open_file(file.node, true),
        Err(MountedError::TooManyOpenFiles)
    ));
    assert!(matches!(
        mounted.create_file(ROOT_NODE, b"partial-create", 0o644),
        Err(MountedError::TooManyOpenFiles)
    ));
    mounted.next_handle = next_handle;
    assert!(matches!(
        mounted.lookup_child(ROOT_NODE, b"partial-create"),
        Err(MountedError::NotFound)
    ));
    let handle = mounted.open_file(file.node, false).unwrap();
    assert_eq!(mounted.read(file.node, handle, 0, 16).unwrap(), b"keep");

    mounted.spool.live = MAX_LIVE_SPOOL_BYTES;
    assert!(matches!(
        mounted.write(file.node, handle, 0, b"x"),
        Err(MountedError::NoSpace)
    ));
    assert_eq!(mounted.spool.physical(), 0);
    mounted.spool.live = 0;
    assert_eq!(mounted.read(file.node, handle, 0, 16).unwrap(), b"keep");
    mounted.release(handle).unwrap();

    mounted
        .dirty_nodes
        .extend((0..MAX_DIRTY_NODES).map(|index| MountedNodeId(u64::MAX - index as u64)));
    assert!(matches!(
        mounted.chmod(file.node, 0o600),
        Err(MountedError::ResourceExhausted)
    ));
    assert_eq!(mounted.getattr(file.node).unwrap().mode, 0o644);
    assert!(matches!(
        mounted.link(file.node, ROOT_NODE, b"partial-link"),
        Err(MountedError::ResourceExhausted)
    ));
    assert!(matches!(
        mounted.rename(ROOT_NODE, b"file", ROOT_NODE, b"partial-rename", false),
        Err(MountedError::ResourceExhausted)
    ));
    assert!(matches!(
        mounted.mknod_file(ROOT_NODE, b"partial-node", 0o644),
        Err(MountedError::ResourceExhausted)
    ));
    mounted.dirty_nodes.clear();
    assert!(mounted.lookup_child(ROOT_NODE, b"file").is_ok());
    for absent in [
        b"partial-link".as_slice(),
        b"partial-rename",
        b"partial-node",
    ] {
        assert!(matches!(
            mounted.lookup_child(ROOT_NODE, absent),
            Err(MountedError::NotFound)
        ));
    }

    mounted.truncate(file.node, MAX_LOGICAL_FILE_BYTES).unwrap();
    let second = mounted
        .mknod_file(ROOT_NODE, b"workspace-limit", 0o644)
        .unwrap();
    let remaining = MAX_LOGICAL_WORKSPACE_BYTES - MAX_LOGICAL_FILE_BYTES;
    assert!(matches!(
        mounted.truncate(second.node, remaining + 1),
        Err(MountedError::NoSpace)
    ));
    assert_eq!(mounted.getattr(second.node).unwrap().size, 0);
    mounted.truncate(file.node, 4).unwrap();
    mounted.unlink(ROOT_NODE, b"workspace-limit").unwrap();

    mounted.shutdown().unwrap();
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}
