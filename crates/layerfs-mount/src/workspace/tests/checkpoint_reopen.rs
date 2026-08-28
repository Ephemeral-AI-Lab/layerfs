#[test]
fn one_checkpoint_reopens_nested_bytes_and_hard_link_identity() {
    let (store, spool, directory) = paths("checkpoint");
    let accepted = {
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x92; 32],
        )
        .unwrap();
        let directory_attr = mounted.mkdir(ROOT_NODE, b"dir", 0o755).unwrap();
        let (file, handle) = mounted
            .create_file(directory_attr.node, b"file", 0o640)
            .unwrap();
        mounted
            .write(file.node, handle, 0, b"persistent bytes")
            .unwrap();
        mounted
            .link(file.node, directory_attr.node, b"alias")
            .unwrap();
        mounted.reset_engine_counters().unwrap();
        let accepted = mounted.fsync().unwrap();
        let engine = mounted.engine_counters().unwrap();
        assert_eq!(engine.transactions_started, 1);
        assert_eq!(engine.transactions_committed, 1);
        assert_eq!(engine.publication_commits, 1);
        mounted.release(handle).unwrap();
        mounted.shutdown().unwrap();
        accepted
    };
    let mut reopened = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x92; 32],
    )
    .unwrap();
    assert_eq!(reopened.accepted(), &accepted);
    let dir = reopened.lookup_child(ROOT_NODE, b"dir").unwrap();
    let file = reopened.lookup_child(dir.node, b"file").unwrap();
    let alias = reopened.lookup_child(dir.node, b"alias").unwrap();
    assert_eq!(file.node, alias.node);
    assert_eq!(file.links, 2);
    let handle = reopened.open_file(file.node, false).unwrap();
    assert_eq!(
        reopened.read(file.node, handle, 0, 64).unwrap(),
        b"persistent bytes"
    );
    reopened.release(handle).unwrap();
    reopened.shutdown().unwrap();
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn later_checkpoints_preserve_untouched_hard_link_content() {
    let (store, spool, directory) = paths("multi-checkpoint");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x94; 32],
    )
    .unwrap();
    let (guard, guard_handle) = mounted.create_file(ROOT_NODE, b"guard", 0o644).unwrap();
    mounted
        .write(guard.node, guard_handle, 0, b"guard-bytes")
        .unwrap();
    mounted.link(guard.node, ROOT_NODE, b"guard-alias").unwrap();
    mounted.fsync().unwrap();
    mounted.release(guard_handle).unwrap();
    let handle = mounted.open_file(guard.node, false).unwrap();
    assert_eq!(
        mounted.read(guard.node, handle, 0, 64).unwrap(),
        b"guard-bytes"
    );
    mounted.release(handle).unwrap();

    let temp = mounted.mkdir(ROOT_NODE, b"temporary", 0o755).unwrap();
    let (child, child_handle) = mounted.create_file(temp.node, b"child", 0o644).unwrap();
    mounted
        .write(child.node, child_handle, 0, b"temporary")
        .unwrap();
    mounted.fsync().unwrap();
    mounted.release(child_handle).unwrap();
    let handle = mounted.open_file(guard.node, false).unwrap();
    assert_eq!(
        mounted.read(guard.node, handle, 0, 64).unwrap(),
        b"guard-bytes"
    );
    mounted.release(handle).unwrap();
    mounted.unlink(temp.node, b"child").unwrap();
    mounted.rmdir(ROOT_NODE, b"temporary").unwrap();
    let (other, other_handle) = mounted.create_file(ROOT_NODE, b"other", 0o644).unwrap();
    mounted
        .write(other.node, other_handle, 0, b"other")
        .unwrap();
    mounted.fsync().unwrap();
    mounted.release(other_handle).unwrap();

    let guard = mounted.lookup_child(ROOT_NODE, b"guard-alias").unwrap();
    let handle = mounted.open_file(guard.node, false).unwrap();
    assert_eq!(
        mounted.read(guard.node, handle, 0, 64).unwrap(),
        b"guard-bytes"
    );
    mounted.release(handle).unwrap();
    mounted.shutdown().unwrap();
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn shrink_then_extend_or_write_never_reveals_truncated_base_bytes() {
    let (store, spool, directory) = paths("truncate-watermark");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x96; 32],
    )
    .unwrap();
    let (extended, extended_handle) = mounted.create_file(ROOT_NODE, b"extended", 0o644).unwrap();
    let (written, written_handle) = mounted.create_file(ROOT_NODE, b"written", 0o644).unwrap();
    mounted
        .write(extended.node, extended_handle, 0, b"abcdef")
        .unwrap();
    mounted
        .write(written.node, written_handle, 0, b"abcdef")
        .unwrap();
    mounted.fsync().unwrap();

    mounted.truncate(extended.node, 2).unwrap();
    mounted.truncate(extended.node, 6).unwrap();
    mounted.truncate(written.node, 2).unwrap();
    mounted
        .write(written.node, written_handle, 4, b"Z")
        .unwrap();
    assert_eq!(
        mounted.read(extended.node, extended_handle, 0, 16).unwrap(),
        b"ab\0\0\0\0"
    );
    assert_eq!(
        mounted.read(written.node, written_handle, 0, 16).unwrap(),
        b"ab\0\0Z"
    );
    mounted.fsync().unwrap();
    mounted.release(extended_handle).unwrap();
    mounted.release(written_handle).unwrap();
    mounted.shutdown().unwrap();
    drop(mounted);

    let mut reopened = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x96; 32],
    )
    .unwrap();
    for (name, expected) in [
        (b"extended".as_slice(), b"ab\0\0\0\0".as_slice()),
        (b"written".as_slice(), b"ab\0\0Z".as_slice()),
    ] {
        let file = reopened.lookup_child(ROOT_NODE, name).unwrap();
        let handle = reopened.open_file(file.node, false).unwrap();
        assert_eq!(reopened.read(file.node, handle, 0, 16).unwrap(), expected);
        reopened.release(handle).unwrap();
    }
    reopened.shutdown().unwrap();
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn repeated_fsync_of_accepted_unlinked_open_orphan_is_exact() {
    let (store, spool, directory) = paths("accepted-orphan");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x97; 32],
    )
    .unwrap();
    let (file, handle) = mounted.create_file(ROOT_NODE, b"orphan", 0o644).unwrap();
    mounted.write(file.node, handle, 0, b"still-open").unwrap();
    mounted.fsync().unwrap();
    mounted.unlink(ROOT_NODE, b"orphan").unwrap();
    let removed = mounted.fsync().unwrap();
    mounted.reset_engine_counters().unwrap();
    mounted
        .write(file.node, handle, 0, b"changed-open")
        .unwrap();
    mounted.truncate(file.node, 7).unwrap();
    mounted.truncate(file.node, 9).unwrap();
    let orphan_only = mounted.fsync().unwrap();
    let repeated = mounted.fsync().unwrap();
    assert_eq!(orphan_only, removed);
    assert_eq!(repeated, removed);
    assert_eq!(
        mounted.read(file.node, handle, 0, 32).unwrap(),
        b"changed\0\0"
    );
    let counters = mounted.engine_counters().unwrap();
    assert_eq!(counters.transactions_started, 0);
    assert_eq!(counters.transactions_committed, 0);
    assert_eq!(counters.transactions_rolled_back, 0);
    assert_eq!(counters.publication_commits, 0);
    mounted.release(handle).unwrap();
    mounted.shutdown().unwrap();
    drop(mounted);

    let mut reopened = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x97; 32],
    )
    .unwrap();
    assert!(matches!(
        reopened.lookup_child(ROOT_NODE, b"orphan"),
        Err(MountedError::NotFound)
    ));
    reopened.shutdown().unwrap();
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn open_orphan_retains_dirty_bytes_through_delete_checkpoint_until_release() {
    for write_before_unlink in [true, false] {
        let label = if write_before_unlink {
            "orphan-write-before-unlink"
        } else {
            "orphan-unlink-before-write"
        };
        let (store, spool, directory) = paths(label);
        let mut mounted = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool.clone(),
            [0x97; 32],
        )
        .unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"orphan", 0o644).unwrap();
        mounted.write(file.node, handle, 0, b"version-one").unwrap();
        mounted.fsync().unwrap();
        if write_before_unlink {
            mounted.write(file.node, handle, 0, b"version-two").unwrap();
            mounted.unlink(ROOT_NODE, b"orphan").unwrap();
        } else {
            mounted.unlink(ROOT_NODE, b"orphan").unwrap();
            mounted.write(file.node, handle, 0, b"version-two").unwrap();
        }
        let removed = mounted.fsync().unwrap();
        assert_eq!(
            mounted.read(file.node, handle, 0, 32).unwrap(),
            b"version-two"
        );
        let retained = mounted.counters().unwrap();
        assert!(retained.dirty_ranges > 0);
        assert!(retained.spool_live_bytes > 0);
        mounted.reset_engine_counters().unwrap();
        assert_eq!(mounted.fsync().unwrap(), removed);
        let engine = mounted.engine_counters().unwrap();
        assert_eq!(engine.transactions_started, 0);
        assert_eq!(engine.transactions_committed, 0);
        assert_eq!(engine.transactions_rolled_back, 0);
        assert_eq!(engine.publication_commits, 0);
        mounted.release(handle).unwrap();
        mounted.forget(file.node, 1);
        let released = mounted.counters().unwrap();
        assert_eq!(released.dirty_ranges, 0);
        assert_eq!(released.spool_live_bytes, 0);
        assert_eq!(released.spool_physical_bytes, 0);
        assert_eq!(released.pending_nodes, 0);
        assert_eq!(released.dirty_nodes, 0);
        assert!(!mounted.nodes.contains_key(&file.node));
        mounted.shutdown().unwrap();
        drop(mounted);

        let mut reopened = MountedWorkspace::open(
            &store,
            "main",
            IntegrityMode::TrustedLocalDev,
            spool,
            [0x97; 32],
        )
        .unwrap();
        assert!(matches!(
            reopened.lookup_child(ROOT_NODE, b"orphan"),
            Err(MountedError::NotFound)
        ));
        reopened.shutdown().unwrap();
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
