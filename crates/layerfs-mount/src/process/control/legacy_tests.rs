#[cfg(any())]
mod legacy_tests {
    use super::*;

    #[test]
    fn lifecycle_coordinator_checkpoints_dirty_workspace_once() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-lifecycle-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let mut workspace = MountedWorkspace::open(
            &root.join("store.sqlite"),
            "main",
            IntegrityMode::TrustedLocalDev,
            root.join("spool"),
            [0xb1; 32],
        )
        .unwrap();
        let (file, handle) = workspace
            .create_file(layerfs_mount::workspace::ROOT_NODE, b"dirty", 0o644)
            .unwrap();
        workspace.write(file.node, handle, 0, b"data").unwrap();
        workspace.release(handle).unwrap();
        let workspace = std::sync::Arc::new(std::sync::Mutex::new(workspace));
        let budget = workspace.lock().unwrap().byte_budget();
        shutdown_workspace(&workspace, &budget).unwrap();
        let mut terminal = workspace.lock().unwrap();
        assert_eq!(terminal.lifecycle(), MountedLifecycle::Closed);
        assert_eq!(terminal.counters().unwrap().checkpoints, 1);
        terminal.close_store_connection().unwrap();
        drop(terminal);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn splice_control_request_is_bounded_and_exact() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-splice-control-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let request = root.join("request");
        std::fs::write(
            &request,
            b"path=dir/file\nstart=7\ndelete=3\nreplacement_hex=00aBff\n",
        )
        .unwrap();
        let parsed = read_splice_request(&request).unwrap();
        assert_eq!(parsed.path_text, "dir/file");
        assert_eq!(parsed.start, 7);
        assert_eq!(parsed.delete_len, 3);
        assert_eq!(parsed.replacement, [0x00, 0xab, 0xff]);
        let mut maximum = b"path=dir/file\nstart=7\ndelete=3\nreplacement_hex=".to_vec();
        maximum.extend("ab".repeat(MAX_REQUEST_BYTES).bytes());
        maximum.push(b'\n');
        std::fs::write(&request, maximum).unwrap();
        assert_eq!(
            read_splice_request(&request).unwrap().replacement.len(),
            MAX_REQUEST_BYTES
        );
        std::fs::write(
            &request,
            b"path=dir/file\nstart=7\ndelete=3\nreplacement_hex=0\n",
        )
        .unwrap();
        assert!(read_splice_request(&request).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
