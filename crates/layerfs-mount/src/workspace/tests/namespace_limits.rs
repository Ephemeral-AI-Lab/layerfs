#[test]
fn mount_wide_directory_changes_and_inode_mappings_are_bounded_and_observed() {
    let (store, spool, directory) = paths("directory-change-cap");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x8d; 32],
    )
    .unwrap();
    let evictable = mounted.mknod_file(ROOT_NODE, b"evictable", 0o644).unwrap();
    mounted.mknod_file(ROOT_NODE, b"victim", 0o644).unwrap();
    mounted.fsync().unwrap();
    let evictable_inode = mounted.nodes[&evictable.node].canonical.unwrap();
    mounted.forget(evictable.node, 1);
    assert!(!mounted.nodes.contains_key(&evictable.node));
    assert!(mounted.by_inode.contains_key(&evictable_inode));
    assert!(mounted
        .reclaimable_inode_mappings
        .contains(&evictable_inode));
    mounted.unlink(ROOT_NODE, b"victim").unwrap();
    let file = mounted.mknod_file(ROOT_NODE, b"source", 0o644).unwrap();
    for index in 2..MAX_DIRECTORY_CHANGES {
        mounted
            .link(file.node, ROOT_NODE, format!("alias-{index}").as_bytes())
            .unwrap();
    }
    let before = mounted.counters().unwrap();
    assert_eq!(before.directory_changes, MAX_DIRECTORY_CHANGES as u64);
    assert_eq!(
        before.directory_changes_high_water,
        MAX_DIRECTORY_CHANGES as u64
    );
    assert!(before.inode_mappings <= MAX_MOUNTED_NODES as u64);
    assert!(matches!(
        mounted.link(file.node, ROOT_NODE, b"one-too-many"),
        Err(MountedError::ResourceExhausted)
    ));
    assert_eq!(mounted.counters().unwrap(), before);
    let node_ids = mounted.nodes.keys().copied().collect::<HashSet<_>>();
    let by_inode = mounted.by_inode.clone();
    let dirty_nodes = mounted.dirty_nodes.clone();
    let pending_nodes = mounted.pending_nodes.clone();
    let engine_counters = mounted.engine_counters().unwrap();
    let scalar_state = (
        mounted.next_node,
        mounted.next_handle,
        mounted.live_ranges,
        mounted.directory_cursors,
        mounted.directory_changes,
        mounted.lookup_refs,
        mounted.logical_workspace_bytes,
        mounted.spool.appended,
        mounted.spool.total_appended,
        mounted.spool.live,
        mounted.spool.physical(),
    );
    let root_state = {
        let root = mounted.nodes.get(&ROOT_NODE).unwrap();
        let NodeContent::Directory { changes, .. } = &root.content else {
            panic!("root is not a directory")
        };
        (
            root.attr(ROOT_NODE),
            root.dirty_content,
            root.dirty_metadata,
            root.dirty_links,
            root.directory_mtime_before,
            changes.clone(),
        )
    };
    assert!(matches!(
        mounted.mknod_file(ROOT_NODE, b"cap-rejected-create", 0o644),
        Err(MountedError::ResourceExhausted)
    ));
    assert_eq!(
        mounted.nodes.keys().copied().collect::<HashSet<_>>(),
        node_ids
    );
    assert_eq!(mounted.by_inode, by_inode);
    assert_eq!(mounted.dirty_nodes, dirty_nodes);
    assert_eq!(mounted.pending_nodes, pending_nodes);
    assert_eq!(
        (
            mounted.next_node,
            mounted.next_handle,
            mounted.live_ranges,
            mounted.directory_cursors,
            mounted.directory_changes,
            mounted.lookup_refs,
            mounted.logical_workspace_bytes,
            mounted.spool.appended,
            mounted.spool.total_appended,
            mounted.spool.live,
            mounted.spool.physical(),
        ),
        scalar_state
    );
    let root = mounted.nodes.get(&ROOT_NODE).unwrap();
    let NodeContent::Directory { changes, .. } = &root.content else {
        panic!("root is not a directory")
    };
    assert_eq!(
        (
            root.attr(ROOT_NODE),
            root.dirty_content,
            root.dirty_metadata,
            root.dirty_links,
            root.directory_mtime_before,
            changes.clone(),
        ),
        root_state
    );
    assert_eq!(mounted.counters().unwrap(), before);
    assert_eq!(mounted.engine_counters().unwrap(), engine_counters);
    mounted.mknod_file(ROOT_NODE, b"victim", 0o644).unwrap();
    assert_eq!(
        mounted.counters().unwrap().directory_changes,
        MAX_DIRECTORY_CHANGES as u64
    );
    let mut nonce = 0_u64;
    while mounted.by_inode.len() < MAX_MOUNTED_NODES {
        let mut bytes = [0_u8; 32];
        bytes[..8].copy_from_slice(&nonce.to_be_bytes());
        mounted
            .by_inode
            .entry(InodeId(bytes))
            .or_insert(MountedNodeId(u64::MAX - nonce));
        nonce += 1;
    }
    let full_mappings = mounted.counters().unwrap();
    assert_eq!(full_mappings.inode_mappings, MAX_MOUNTED_NODES as u64);
    assert!(matches!(
        mounted.load_canonical_node(InodeId([0xff; 32]), MountedNodeId(u64::MAX / 2)),
        Err(MountedError::Corrupt)
    ));
    assert!(!mounted.by_inode.contains_key(&evictable_inode));
    let mappings = mounted.counters().unwrap();
    assert_eq!(mappings.inode_mappings, MAX_MOUNTED_NODES as u64 - 1);
    assert_eq!(mappings.inode_mappings_high_water, MAX_MOUNTED_NODES as u64);
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn replacement_rename_reclaims_directory_changes_after_checkpoint() {
    let (store, spool, directory) = paths("replacement-rename");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x8d; 32],
    )
    .unwrap();
    let (source, source_handle) = mounted.create_file(ROOT_NODE, b"source", 0o644).unwrap();
    let (_, target_handle) = mounted.create_file(ROOT_NODE, b"target", 0o644).unwrap();
    mounted
        .write(source.node, source_handle, 0, b"same-directory")
        .unwrap();
    mounted.release(source_handle).unwrap();
    mounted.release(target_handle).unwrap();
    mounted.fsync().unwrap();
    mounted
        .rename(ROOT_NODE, b"source", ROOT_NODE, b"target", false)
        .unwrap();
    mounted.fsync().unwrap();
    assert_eq!(mounted.counters().unwrap().directory_changes, 0);
    assert!(matches!(
        mounted.lookup_child(ROOT_NODE, b"source"),
        Err(MountedError::NotFound)
    ));
    let target = mounted.lookup_child(ROOT_NODE, b"target").unwrap();
    let handle = mounted.open_file(target.node, false).unwrap();
    assert_eq!(
        mounted.read(target.node, handle, 0, 32).unwrap(),
        b"same-directory"
    );
    mounted.release(handle).unwrap();

    let left = mounted.mkdir(ROOT_NODE, b"left", 0o755).unwrap();
    let right = mounted.mkdir(ROOT_NODE, b"right", 0o755).unwrap();
    let (source, source_handle) = mounted.create_file(left.node, b"source", 0o644).unwrap();
    let (_, target_handle) = mounted.create_file(right.node, b"target", 0o644).unwrap();
    mounted
        .write(source.node, source_handle, 0, b"cross-directory")
        .unwrap();
    mounted.release(source_handle).unwrap();
    mounted.release(target_handle).unwrap();
    mounted.fsync().unwrap();
    mounted
        .rename(left.node, b"source", right.node, b"target", false)
        .unwrap();
    mounted.fsync().unwrap();
    assert_eq!(mounted.counters().unwrap().directory_changes, 0);
    assert!(matches!(
        mounted.lookup_child(left.node, b"source"),
        Err(MountedError::NotFound)
    ));
    let target = mounted.lookup_child(right.node, b"target").unwrap();
    let handle = mounted.open_file(target.node, false).unwrap();
    assert_eq!(
        mounted.read(target.node, handle, 0, 32).unwrap(),
        b"cross-directory"
    );
    mounted.release(handle).unwrap();
    mounted.shutdown().unwrap();
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn post_unmount_cache_release_is_root_only_and_logically_empty() {
    let (store, spool, directory) = paths("terminal-cache-release");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x8d; 32],
    )
    .unwrap();
    let file = mounted.mknod_file(ROOT_NODE, b"temporary", 0o644).unwrap();
    mounted.fsync().unwrap();
    mounted.unlink(ROOT_NODE, b"temporary").unwrap();
    mounted.fsync().unwrap();
    assert!(mounted.nodes.contains_key(&file.node));
    mounted.shutdown().unwrap();
    mounted.release_kernel_cache_ownership().unwrap();
    let counters = mounted.counters().unwrap();
    assert_eq!(counters.lookup_refs, 1);
    assert_eq!(counters.live_nodes, 1);
    assert_eq!(counters.inode_mappings, 1);
    assert_eq!(counters.open_handles, 0);
    assert_eq!(counters.pending_nodes, 0);
    assert_eq!(counters.dirty_nodes, 0);
    assert_eq!(counters.dirty_ranges, 0);
    assert_eq!(counters.directory_cursors, 0);
    assert_eq!(counters.directory_changes, 0);
    assert_eq!(counters.logical_workspace_bytes, 0);
    assert_eq!(counters.spool_live_bytes, 0);
    assert_eq!(counters.spool_physical_bytes, 0);
    assert_eq!(counters.operation_q_current_bytes, 0);
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}
