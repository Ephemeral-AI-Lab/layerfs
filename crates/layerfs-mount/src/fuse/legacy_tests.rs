#[cfg(any())]
mod legacy_tests {
    use super::*;
    use layerfs_storage::integrity::IntegrityMode;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fuse(label: &str) -> (LayerFuse, PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-mount-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let workspace = MountedWorkspace::open(
            &directory.join("store.sqlite"),
            "main",
            IntegrityMode::TrustedLocalDev,
            directory.join("spool"),
            [0xa1; 32],
        )
        .unwrap();
        (LayerFuse::new(workspace, 0, 0), directory)
    }

    #[test]
    fn missing_entry_and_inode_notifier_fail_closed() {
        let (fuse, directory) = fuse("missing-notifier");
        assert_eq!(
            fuse.invalidate_entry(INodeNo::ROOT, OsStr::new("entry")),
            Err(fuser::Errno::EIO)
        );
        assert_eq!(
            fuse.invalidate_inode(INodeNo(2), 0, 1),
            Err(fuser::Errno::EIO)
        );
        assert_eq!(
            fuse.shared_workspace().lock().unwrap().lifecycle(),
            crate::workspace::MountedLifecycle::Incomplete
        );
        let counters = *fuse.shared_counters().lock().unwrap();
        assert_eq!(counters.invalidations_requested, 2);
        assert_eq!(counters.invalidations_unsupported, 2);
        drop(fuse);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn notifier_success_and_failure_counters_are_exact_and_failure_is_incomplete() {
        let (fuse, directory) = fuse("notifier-outcomes");
        fuse.count(|counters| counters.invalidations_requested += 2);
        assert_eq!(fuse.finish_invalidation(Ok(())), Ok(()));
        assert_eq!(
            fuse.finish_invalidation(Err(std::io::Error::other("injected"))),
            Err(fuser::Errno::EIO)
        );
        let counters = *fuse.shared_counters().lock().unwrap();
        assert_eq!(counters.invalidations_requested, 2);
        assert_eq!(counters.invalidations_succeeded, 1);
        assert_eq!(counters.invalidations_failed, 1);
        assert_eq!(
            fuse.shared_workspace().lock().unwrap().lifecycle(),
            crate::workspace::MountedLifecycle::Incomplete
        );
        drop(fuse);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn callback_wall_includes_workspace_lock_wait() {
        let (fuse, directory) = fuse("callback-wall");
        {
            let _callback = fuse.callback();
            let workspace = fuse.lock().unwrap();
            drop(workspace);
        }
        let counters = *fuse.shared_counters().lock().unwrap();
        assert!(counters.callback_wall_ns > 0);
        assert!(counters.mount_lock_wait_ns <= counters.callback_wall_ns);
        drop(fuse);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn destroy_only_emits_one_session_end_event() {
        let (mut fuse, directory) = fuse("destroy-event");
        let workspace = fuse.shared_workspace();
        let (sender, receiver) = std::sync::mpsc::channel();
        fuse.set_lifecycle_sender(sender).unwrap();
        {
            let mut workspace = workspace.lock().unwrap();
            let (file, handle) = workspace.create_file(ROOT_NODE, b"dirty", 0o644).unwrap();
            workspace.write(file.node, handle, 0, b"data").unwrap();
            workspace.release(handle).unwrap();
        }
        Filesystem::destroy(&mut fuse);
        Filesystem::destroy(&mut fuse);
        assert_eq!(receiver.recv().unwrap(), LayerFuseEvent::SessionEnded);
        assert!(matches!(
            receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        assert_eq!(
            workspace.lock().unwrap().lifecycle(),
            crate::workspace::MountedLifecycle::Live
        );
        workspace.lock().unwrap().shutdown().unwrap();
        drop(workspace);
        drop(fuse);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
