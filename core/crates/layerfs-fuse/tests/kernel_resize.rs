//! Mounted size SETATTR and a separately declared native projection-origin subset.
#[cfg(target_os = "linux")]
#[path = "../../layerfs-workspace/tests/support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::*;
    use layerfs_workspace::*;
    use std::{
        ffi::CString,
        fs::{self, File, OpenOptions},
        io::Write,
        os::unix::{
            ffi::OsStrExt,
            fs::{FileExt, MetadataExt, OpenOptionsExt},
        },
        process::Command,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };
    fn check(id: &str) {
        println!("KERNEL_RESIZE_CHECK {id} PASS");
    }
    fn quiescent(f: &Fixture) {
        let end = deadline();
        while f.workspace.status().unwrap().projection_replies != 0 {
            assert!(Instant::now() < end);
            std::thread::yield_now();
        }
    }
    fn drained(f: &Fixture) {
        let end = deadline();
        loop {
            let status = f.workspace.status().unwrap();
            if status.projection_replies == 0 && status.projection_handles == 0 {
                break;
            }
            assert!(Instant::now() < end, "{status:?}");
            std::thread::yield_now();
        }
    }
    fn data(f: &Fixture, length: Option<u64>) -> NodeAttributes {
        let data = f.lookup(b"data.bin");
        if let Some(length) = length {
            f.workspace
                .set_len(data.serial, length, deadline())
                .unwrap();
        }
        f.workspace.getattr(data.serial).unwrap()
    }
    fn open(f: &Fixture, name: &str) -> File {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(f.workspace.mount_path().join(name))
            .unwrap()
    }
    fn read(file: &File, start: u64, length: usize) -> Vec<u8> {
        let mut bytes = vec![0; length];
        file.read_exact_at(&mut bytes, start).unwrap();
        bytes
    }
    fn path(f: &Fixture) -> CString {
        CString::new(
            f.workspace
                .mount_path()
                .join("data.bin")
                .as_os_str()
                .as_bytes(),
        )
        .unwrap()
    }
    fn saved(f: &Fixture) -> (Root, (u64, Root, u64, i64, u32)) {
        let report = f.workspace.commit(deadline()).unwrap();
        let root = match report.outcome {
            CommitOutcomeWire::Committed(c) => c.root,
            other => panic!("{other:?}"),
        };
        (root, attr(f.native.attributes(root, b"data.bin")))
    }
    fn observe(f: &Fixture) {
        println!(
            "RESIZE_RESOURCE {:?} {:?} {:?}",
            f.workspace.status().unwrap(),
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
    }
    fn finish(f: &Fixture, data: NodeAttributes, mount: &mut layerfs_fuse::MountHandle) {
        drained(f);
        mount.unmount(deadline()).unwrap();
        assert!(!fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .any(|line| line.split(' ').nth(4) == f.workspace.mount_path().to_str()));
        observe(f);
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
    }
    fn local(f: &Fixture, serial: u64, scope: ReferenceScope, access: FileAccess) -> HandleId {
        f.workspace
            .open_file(
                serial,
                FileOpenOptions {
                    access,
                    append: false,
                    truncate: false,
                },
                scope,
                deadline(),
            )
            .unwrap()
    }
    fn unchanged(f: &Fixture, expected: NodeAttributes, revision: u64, dirty: usize) {
        assert_eq!(f.workspace.getattr(expected.serial).unwrap(), expected);
        let state = f.workspace.status().unwrap();
        assert_eq!((state.revision, state.dirty_inodes), (revision, dirty));
    }

    #[test]
    #[ignore = "requires real privileged Linux FUSE and native service"]
    fn kernel_resize_semantics() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, Some(8192));
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = open(&f, "data.bin");
        let alias = open(&f, "alias");
        assert_eq!(
            file.metadata().unwrap().ino(),
            alias.metadata().unwrap().ino()
        );
        assert_eq!(
            read(&file, 4088, 16),
            (4088..4104).map(|i| (i % 251) as u8).collect::<Vec<_>>()
        );
        assert_eq!(unsafe { libc::truncate(path(&f).as_ptr(), 4093) }, 0);
        assert_eq!(file.metadata().unwrap().len(), 4093);
        alias.set_len(8200).unwrap();
        let mut expected: Vec<_> = (0..4093).map(|i| (i % 251) as u8).collect();
        expected.resize(8200, 0);
        assert_eq!(read(&file, 0, 8200), expected);
        assert_eq!(read(&alias, 0, 8200), expected);
        let live = f.workspace.getattr(data.serial).unwrap();
        let stat = file.metadata().unwrap();
        assert_eq!(
            (stat.len(), stat.mtime(), stat.mtime_nsec()),
            (
                live.size,
                live.mtime_seconds,
                i64::from(live.mtime_nanoseconds)
            )
        );
        let (root, first) = saved(&f);
        assert_eq!(first.2, 8200);
        assert_eq!(f.native.bytes(first.1, 0, 8200), expected);
        assert_eq!(attr(f.native.attributes(root, b"alias")), first);
        file.set_len(0).unwrap();
        alias.set_len(40).unwrap();
        assert_eq!(read(&file, 0, 40), vec![0; 40]);
        let (_, second) = saved(&f);
        assert_eq!(second.2, 40);
        assert_eq!(f.native.bytes(second.1, 0, 40), vec![0; 40]);
        assert_eq!(f.workspace.backing_status().unwrap().payloads, 0);
        drop(file);
        drop(alias);
        finish(&f, data, &mut mount);
        check("mounted-path-and-fd-shrink-reextend-preserve-aliases-and-incremental-Commits");
    }

    #[test]
    #[ignore = "requires actual Linux OPEN then size SETATTR and shell redirection"]
    fn kernel_resize_open_trunc() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, Some(128));
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let alias = File::open(f.workspace.mount_path().join("alias")).unwrap();
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(f.workspace.mount_path().join("data.bin"))
            .unwrap();
        assert_eq!(file.metadata().unwrap().len(), 0);
        assert_eq!(alias.metadata().unwrap().len(), 0);
        let mut byte = [0];
        assert_eq!(alias.read_at(&mut byte, 0).unwrap(), 0);
        assert_eq!(f.workspace.getattr(data.serial).unwrap().size, 0);
        file.write_all(b"new").unwrap();
        assert_eq!(read(&alias, 0, 3), b"new");
        drop(file);
        let (_, first) = saved(&f);
        assert_eq!(f.native.bytes(first.1, 0, 3), b"new");
        assert!(Command::new("sh")
            .arg("-c")
            .arg(": > \"$1\"")
            .arg("sh")
            .arg(f.workspace.mount_path().join("data.bin"))
            .status()
            .unwrap()
            .success());
        assert_eq!(alias.metadata().unwrap().len(), 0);
        assert_eq!(alias.read_at(&mut byte, 0).unwrap(), 0);
        let (_, second) = saved(&f);
        assert_eq!(second.2, 0);
        drop(alias);
        finish(&f, data, &mut mount);
        check("existing-file-O_TRUNC-and-shell-redirection-publish-empty-at-open");
    }

    #[test]
    #[ignore = "requires actual same-length ftruncate and native metadata-only Commit"]
    fn kernel_resize_same_length() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, None);
        let Response::History(branch) = f.branch() else {
            panic!("branch missing")
        };
        let HistoryResult::BranchSnapshot(branch) = *branch else {
            panic!("snapshot missing")
        };
        let original = attr(f.native.attributes(branch.effective_root, b"data.bin"));
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = open(&f, "data.bin");
        quiescent(&f);
        let before = f.workspace.status().unwrap();
        file.set_len(data.size).unwrap();
        let after = f.workspace.status().unwrap();
        assert_eq!(after.revision, before.revision + 1);
        assert_eq!(after.dirty_inodes, 1);
        let live = f.workspace.getattr(data.serial).unwrap();
        let stat = file.metadata().unwrap();
        assert_eq!(
            (stat.mtime(), stat.mtime_nsec()),
            (live.mtime_seconds, i64::from(live.mtime_nanoseconds))
        );
        let (_, committed) = saved(&f);
        assert_eq!((committed.1, committed.2), (original.1, original.2));
        assert_eq!(
            (committed.3, committed.4),
            (live.mtime_seconds, live.mtime_nanoseconds)
        );
        assert!(!f
            .native
            .observations
            .lock()
            .unwrap()
            .operations
            .iter()
            .any(|op| matches!(op, Operation::SaveFile { base: Some(_), .. })));
        assert_eq!(read(&file, 0, 4), [0, 1, 2, 3]);
        drop(file);
        finish(&f, data, &mut mount);
        check("mounted-same-length-resize-saves-mtime-without-content-upload");
    }

    #[test]
    #[ignore = "requires real kernel metadata refusals and OPEN flag validation"]
    fn kernel_resize_refused() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, None);
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = open(&f, "data.bin");
        quiescent(&f);
        let before = f.workspace.status().unwrap();
        let name = path(&f);
        let times = [libc::timespec {
            tv_sec: 1234,
            tv_nsec: 5678,
        }; 2];
        let refused = |result| {
            let error = std::io::Error::last_os_error();
            assert_eq!(result, -1);
            assert_eq!(error.raw_os_error(), Some(libc::EOPNOTSUPP));
        };
        refused(unsafe { libc::chmod(name.as_ptr(), 0o600) });
        refused(unsafe { libc::chown(name.as_ptr(), 1, 1) });
        refused(unsafe { libc::utimensat(libc::AT_FDCWD, name.as_ptr(), times.as_ptr(), 0) });
        unchanged(&f, data, before.revision, before.dirty_inodes);
        for flags in [libc::O_SYNC, libc::O_DSYNC, libc::O_DIRECT] {
            let error = OpenOptions::new()
                .write(true)
                .truncate(true)
                .custom_flags(flags)
                .open(f.workspace.mount_path().join("data.bin"))
                .unwrap_err();
            assert_eq!(error.raw_os_error(), Some(libc::EOPNOTSUPP));
            unchanged(&f, data, before.revision, before.dirty_inodes);
        }
        let readonly = File::open(f.workspace.mount_path().join("alias")).unwrap();
        let error = readonly.set_len(0).unwrap_err();
        assert!(
            matches!(error.raw_os_error(), Some(libc::EINVAL | libc::EBADF)),
            "{error:?}"
        );
        unchanged(&f, data, before.revision, before.dirty_inodes);
        assert_eq!(read(&file, 0, 4), [0, 1, 2, 3]);
        drop(readonly);
        drop(file);
        finish(&f, data, &mut mount);
        check("unsupported-metadata-open-flags-and-readonly-ftruncate-preserve-visible-state");
    }

    #[test]
    #[ignore = "requires successful OPEN followed by quota-refused kernel SETATTR"]
    fn kernel_resize_failed_open() {
        let f = Fixture::with_quota(Gate::None, 1024 * 1024);
        let data = data(&f, None);
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let before = f.workspace.status().unwrap();
        let error = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(f.workspace.mount_path().join("data.bin"))
            .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::ENOSPC));
        drained(&f);
        unchanged(&f, data, before.revision, before.dirty_inodes);
        let file = File::open(f.workspace.mount_path().join("alias")).unwrap();
        assert_eq!(file.metadata().unwrap().len(), data.size);
        assert_eq!(read(&file, 0, 4), [0, 1, 2, 3]);
        drop(file);
        finish(&f, data, &mut mount);
        check("failed-truncating-open-preserves-original-version-and-drains-projection-handle");
    }

    struct RestoreLimit;
    impl Drop for RestoreLimit {
        fn drop(&mut self) {
            file_limit("unlimited");
        }
    }
    #[test]
    #[ignore = "requires real private metadata allocation failure under prlimit"]
    fn kernel_resize_metadata_failure() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, None);
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = open(&f, "data.bin");
        quiescent(&f);
        let before = f.workspace.status().unwrap();
        let restore = RestoreLimit;
        file_limit("2048");
        let error = file.set_len(128).unwrap_err();
        drop(restore);
        assert_eq!(error.raw_os_error(), Some(libc::EIO));
        unchanged(&f, data, before.revision, before.dirty_inodes);
        assert_eq!(file.metadata().unwrap().len(), data.size);
        assert_eq!(read(&file, 120, 16), (120u8..136).collect::<Vec<_>>());
        let metadata = f.workspace.metadata_status().unwrap();
        assert!(metadata.allocated_bytes > 0 || metadata.reserved_bytes > 0);
        assert!(metadata.roots > 0);
        assert_eq!(f.workspace.backing_status().unwrap().payloads, 0);
        println!("EXPECTED_METADATA_FAILURE {error:?} {metadata:?}");
        drop(file);
        drained(&f);
        mount.unmount(deadline()).unwrap();
        observe(&f);
        assert!(!f.workspace.status().unwrap().mounted);
        assert_eq!(
            f.workspace.metadata_status().unwrap().allocated_bytes,
            metadata.allocated_bytes
        );
        check("mounted-size-metadata-failure-preserves-version-and-retains-accounted-owner");
    }

    #[test]
    #[ignore = "requires mounted exact zero-input envelope and real bounded Commit"]
    fn kernel_resize_envelope() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, None);
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = open(&f, "data.bin");
        file.set_len(0).unwrap();
        file.set_len(8 * 1024 * 1024).unwrap();
        let attrs = f.workspace.getattr(data.serial).unwrap();
        let before = f.workspace.status().unwrap();
        assert_eq!(
            file.set_len(8 * 1024 * 1024 + 1)
                .unwrap_err()
                .raw_os_error(),
            Some(libc::ENOSPC)
        );
        unchanged(&f, attrs, before.revision, before.dirty_inodes);
        assert_eq!(file.metadata().unwrap().len(), 8 * 1024 * 1024);
        assert_eq!(read(&file, 8 * 1024 * 1024 - 64, 64), vec![0; 64]);
        let backing = f.workspace.backing_status().unwrap();
        assert_eq!(backing.payloads, 0);
        assert!(backing.allocated_bytes < 128 * 1024);
        let (_, committed) = saved(&f);
        assert_eq!(committed.2, 8 * 1024 * 1024);
        for start in (0..committed.2).step_by(MAX_READ_BYTES) {
            assert!(f
                .native
                .bytes(committed.1, start, MAX_READ_BYTES)
                .iter()
                .all(|b| *b == 0));
        }
        assert_eq!(f.workspace.backing_status().unwrap().payloads, 0);
        drop(file);
        finish(&f, data, &mut mount);
        check("mounted-zero-extension-exact-eight-MiB-bound-without-payload-allocation");
    }

    #[test]
    #[ignore = "requires actual C2 RESERVED-lock observer and mounted resize progress"]
    fn kernel_resize_native_save() {
        let f = Fixture::new(Gate::NativeSave);
        let data = data(&f, None);
        let mut bytes = vec![0; 4 * 1024 * 1024];
        let mut random = 0x6a09e667f3bcc909u64;
        for byte in &mut bytes {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            *byte = random as u8;
        }
        f.edit(b"data.bin", 0, bytes.len() as u64, &bytes);
        f.workspace
            .set_len(data.serial, bytes.len() as u64, deadline())
            .unwrap();
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = open(&f, "data.bin");
        assert_eq!(read(&file, 0, bytes.len()), bytes);
        quiescent(&f);
        let ws = f.workspace.clone();
        let saving = std::thread::spawn(move || ws.commit(deadline()));
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "SAVE_PAUSED");
        let shortened = bytes.len() as u64 - 64;
        file.set_len(shortened).unwrap();
        file.set_len(bytes.len() as u64 + 32).unwrap();
        assert_eq!(read(&file, shortened, 96), vec![0; 96]);
        println!("STAGE_LIVE_READY");
        std::io::stdout().flush().unwrap();
        let report = saving.join().unwrap().unwrap();
        let root = match report.outcome {
            CommitOutcomeWire::Committed(c) => c.root,
            other => panic!("{other:?}"),
        };
        let frozen = attr(f.native.attributes(root, b"data.bin"));
        assert_eq!(frozen.2, bytes.len() as u64);
        assert_eq!(
            f.native.bytes(frozen.1, shortened, 64),
            bytes[shortened as usize..]
        );
        assert_eq!(
            f.native.bytes(frozen.1, 0, MAX_READ_BYTES),
            bytes[..MAX_READ_BYTES]
        );
        let (_, live) = saved(&f);
        assert_eq!(live.2, bytes.len() as u64 + 32);
        assert_eq!(f.native.bytes(live.1, shortened, 96), vec![0; 96]);
        assert_eq!(read(&file, shortened, 96), vec![0; 96]);
        drop(file);
        finish(&f, data, &mut mount);
        check("mounted-resize-and-zero-read-progress-during-actual-C2-save");
    }

    #[test]
    #[ignore = "native public projection-origin subset; no kernel mount"]
    fn kernel_resize_origin() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, Some(128));
        let h = local(
            &f,
            data.serial,
            ReferenceScope::Local,
            FileAccess::ReadWrite,
        );
        let other = f.lookup(b"other.bin");
        let mut lease = f.workspace.reserve_mount().unwrap();
        let notifications = Arc::new(AtomicUsize::new(0));
        let count = notifications.clone();
        lease
            .bind_invalidation(Arc::new(move |_, _, _| {
                count.fetch_add(1, Ordering::SeqCst);
                Err(std::io::Error::from_raw_os_error(libc::EIO))
            }))
            .unwrap();
        let projected = local(
            &f,
            data.serial,
            ReferenceScope::Projection,
            FileAccess::ReadWrite,
        );
        let readonly = local(
            &f,
            data.serial,
            ReferenceScope::Projection,
            FileAccess::ReadOnly,
        );
        let wrong_inode = local(
            &f,
            other.serial,
            ReferenceScope::Projection,
            FileAccess::ReadWrite,
        );
        let stale = local(
            &f,
            data.serial,
            ReferenceScope::Projection,
            FileAccess::ReadWrite,
        );
        f.workspace.release(stale).unwrap();
        let before = f.workspace.status().unwrap();
        for (serial, handle) in [
            (data.serial, Some(h)),
            (data.serial, Some(readonly)),
            (data.serial, Some(wrong_inode)),
            (data.serial, Some(stale)),
            (f.workspace.root().serial, None),
            (u64::MAX, None),
        ] {
            let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
            assert!(permit.set_len(serial, 64, handle, deadline()).is_err());
            assert_eq!(
                permit.set_len(data.serial, 64, None, deadline()),
                Err(WorkspaceError::InvalidInput)
            );
            unchanged(&f, data, before.revision, before.dirty_inodes);
        }
        let observer = f.workspace.begin_projection_reply(deadline()).unwrap();
        assert!(matches!(
            f.workspace.begin_projection_mutation(deadline()),
            Err(WorkspaceError::Busy)
        ));
        drop(observer);
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        let observer = f.workspace.begin_projection_reply(deadline()).unwrap();
        assert_eq!(
            permit.set_len(data.serial, 64, Some(projected), deadline()),
            Err(WorkspaceError::Busy)
        );
        unchanged(&f, data, before.revision, before.dirty_inodes);
        drop(observer);
        drop(permit);
        assert!(matches!(
            f.workspace
                .begin_projection_mutation(Instant::now() - Duration::from_secs(1)),
            Err(WorkspaceError::Deadline)
        ));
        let end = Instant::now() + Duration::from_millis(10);
        let mut permit = f.workspace.begin_projection_mutation(end).unwrap();
        while Instant::now() < end {
            std::thread::yield_now();
        }
        assert_eq!(
            permit.set_len(data.serial, 64, None, deadline()),
            Err(WorkspaceError::Deadline)
        );
        drop(permit);
        unchanged(&f, data, before.revision, before.dirty_inodes);
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        let resized = permit
            .set_len(data.serial, 96, Some(projected), deadline())
            .unwrap();
        assert_eq!(resized, f.workspace.getattr(data.serial).unwrap());
        assert_eq!(resized.size, 96);
        assert_eq!(f.workspace.status().unwrap().projection_replies, 1);
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Ready)
        );
        assert_eq!(
            f.workspace.set_len(data.serial, 32, deadline()),
            Err(WorkspaceError::Busy)
        );
        assert_eq!(
            permit.set_len(data.serial, 32, None, deadline()),
            Err(WorkspaceError::InvalidInput)
        );
        assert_eq!(f.read(h, 92, 4), [92, 93, 94, 95]);
        drop(permit);
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        assert_eq!(
            permit
                .set_len(data.serial, 160, None, deadline())
                .unwrap()
                .size,
            160
        );
        assert_eq!(f.read(h, 96, 64), vec![0; 64]);
        drop(permit);
        assert_eq!(notifications.load(Ordering::SeqCst), 0);
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Ready)
        );
        for handle in [projected, readonly, wrong_inode] {
            f.workspace.release(handle).unwrap();
        }
        lease.finish().unwrap();
        let (_, committed) = saved(&f);
        assert_eq!(committed.2, 160);
        assert_eq!(f.native.bytes(committed.1, 96, 64), vec![0; 64]);
        f.workspace.release(h).unwrap();
        for attr in [data, other] {
            f.workspace
                .forget(attr.serial, u64::MAX, ReferenceScope::Local);
        }
        f.workspace.close_clean().unwrap();
        check(
            "projection-size-origin-skips-notifier-and-keeps-identity-deadline-and-reply-contract",
        );
    }
}
