#[test]
fn pending_directory_cursor_is_complete_and_resumable() {
    let (store, spool, directory) = paths("readdir");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x93; 32],
    )
    .unwrap();
    let dir = mounted.mkdir(ROOT_NODE, b"dir", 0o755).unwrap();
    for index in 0..300 {
        mounted
            .mknod_file(dir.node, format!("file-{index:03}").as_bytes(), 0o644)
            .unwrap();
    }
    let handle = mounted.open_directory(dir.node).unwrap();
    let mut offset = 0;
    let mut names = Vec::new();
    loop {
        let entries = mounted.readdir(handle, offset, 19).unwrap();
        if entries.is_empty() {
            break;
        }
        offset = entries.last().unwrap().next_offset;
        names.extend(entries.into_iter().map(|entry| entry.name));
    }
    assert_eq!(names.len(), 302);
    assert_eq!(names[0], b".");
    assert_eq!(names[1], b"..");
    assert!(names[2..].windows(2).all(|pair| pair[0] < pair[1]));
    mounted.release(handle).unwrap();
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn readdir_pending_entry_reclaims_nodes_without_changing_inode_identity() {
    let (store, spool, directory) = paths("readdir-reclaim");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x9c; 32],
    )
    .unwrap();
    let file = mounted.mknod_file(ROOT_NODE, b"file", 0o644).unwrap();
    mounted.fsync().unwrap();
    mounted.forget(file.node, 1);
    let handle = mounted.open_directory(ROOT_NODE).unwrap();
    assert_eq!(mounted.readdir(handle, 0, 2).unwrap().len(), 2);
    let pending = mounted.readdir_next(handle, 2).unwrap().unwrap();
    let emitted = pending.node;
    assert_eq!(pending.name, b"file");
    mounted.discard_readdir_pending(handle).unwrap();
    mounted.reclaim_readdir_nodes(&[emitted]);
    assert!(!mounted.nodes.contains_key(&emitted));
    let replayed = mounted.readdir_next(handle, 2).unwrap().unwrap();
    assert_eq!(replayed.node, emitted);
    mounted
        .commit_readdir(handle, replayed.next_offset)
        .unwrap();
    mounted.reclaim_readdir_nodes(&[emitted]);
    let looked_up = mounted.lookup_child(ROOT_NODE, b"file").unwrap();
    assert_eq!(looked_up.node, emitted);
    mounted.release(handle).unwrap();
    mounted.shutdown().unwrap();
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cold_lookup_and_returned_attrs_stay_inside_the_sql_gate() {
    let (store, spool, directory) = paths("cold-lookup-gate");
    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool.clone(),
        [0x9c; 32],
    )
    .unwrap();
    mounted.mknod_file(ROOT_NODE, b"file", 0o640).unwrap();
    mounted.fsync().unwrap();
    mounted.shutdown().unwrap();
    drop(mounted);

    let mut mounted = MountedWorkspace::open(
        &store,
        "main",
        IntegrityMode::TrustedLocalDev,
        spool,
        [0x9c; 32],
    )
    .unwrap();
    mounted.reset_engine_counters().unwrap();
    let file = mounted.lookup_child(ROOT_NODE, b"file").unwrap();
    assert_eq!(file.mode, 0o640);
    let counters = mounted.engine_counters().unwrap();
    eprintln!("cold_lookup_primary_sql_statements={}", counters.statements);
    assert!(
        counters.statements <= MAX_COLD_LOOKUP_PRIMARY_STATEMENTS,
        "cold lookup used {} primary SQL statements",
        counters.statements
    );
    mounted.shutdown().unwrap();
    drop(mounted);
    std::fs::remove_dir_all(directory).unwrap();
}
