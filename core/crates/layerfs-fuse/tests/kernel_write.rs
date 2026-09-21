//! Actual mounted WRITE routes and separately declared native origin/error contracts.
#[cfg(target_os = "linux")]
#[path = "../../layerfs-workspace/tests/support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::*;
    use layerfs_workspace::*;
    use std::{
        fs::{self, File, OpenOptions},
        io::{Seek, Write},
        os::{
            fd::AsRawFd,
            unix::fs::{FileExt, MetadataExt, OpenOptionsExt},
        },
        process::Command,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Barrier,
        },
        time::{Duration, Instant},
    };
    fn check(id: &str) {
        println!("KERNEL_WRITE_CHECK {id} PASS");
    }
    fn quiescent(f: &Fixture) {
        let end = deadline();
        while f.workspace.status().unwrap().projection_replies != 0 {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn data(f: &Fixture, length: Option<u64>) -> NodeAttributes {
        let data = f.lookup(b"data.bin");
        if let Some(length) = length {
            f.workspace
                .set_len(data.serial, length, deadline())
                .unwrap();
        }
        data
    }
    fn open_mounted(f: &Fixture, name: &str, append: bool) -> File {
        OpenOptions::new()
            .read(true)
            .write(true)
            .append(append)
            .open(f.workspace.mount_path().join(name))
            .unwrap()
    }
    fn read(file: &File, start: u64, length: usize) -> Vec<u8> {
        let mut bytes = vec![0; length];
        file.read_exact_at(&mut bytes, start).unwrap();
        bytes
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
            "WRITE_RESOURCE {:?} {:?} {:?}",
            f.workspace.status().unwrap(),
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
    }
    fn finish(f: &Fixture, data: NodeAttributes, mount: &mut layerfs_fuse::MountHandle) {
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
    fn local(f: &Fixture, data: NodeAttributes, scope: ReferenceScope, append: bool) -> HandleId {
        f.workspace
            .open_file(
                data.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    append,
                    truncate: false,
                },
                scope,
                deadline(),
            )
            .unwrap()
    }

    #[test]
    #[ignore = "requires real privileged Linux FUSE and native service"]
    fn kernel_write_positional() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, Some(8192));
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = open_mounted(&f, "data.bin", false);
        let alias = open_mounted(&f, "alias", false);
        assert_eq!(
            file.metadata().unwrap().ino(),
            alias.metadata().unwrap().ino()
        );
        let mut expected: Vec<_> = (0..8192).map(|i| (i % 251) as u8).collect();
        file.write_all_at(b"ABCDEFG", 4093).unwrap();
        expected[4093..4100].copy_from_slice(b"ABCDEFG");
        alias.write_all_at(b"TAIL", 9000).unwrap();
        expected.resize(9004, 0);
        expected[9000..].copy_from_slice(b"TAIL");
        assert_eq!(read(&file, 0, 9004), expected);
        assert_eq!(read(&alias, 0, 9004), expected);
        let attr = f.workspace.getattr(data.serial).unwrap();
        let stat = alias.metadata().unwrap();
        assert_eq!(
            (stat.len(), stat.mtime(), stat.mtime_nsec()),
            (9004, attr.mtime_seconds, attr.mtime_nanoseconds as i64)
        );
        quiescent(&f);
        let before = f.workspace.status().unwrap().revision;
        assert_eq!(file.write_at(b"", 1).unwrap(), 0);
        assert_eq!(f.workspace.status().unwrap().revision, before);
        for bytes in [b'A', b'B', b'A'] {
            file.write_all_at(&[bytes], 0).unwrap();
            expected[0] = bytes;
            let (root, committed) = saved(&f);
            assert_eq!(committed.2, expected.len() as u64);
            assert_eq!(f.native.bytes(committed.1, 0, expected.len()), expected);
            assert_eq!(
                super::support::attr(f.native.attributes(root, b"alias")).1,
                committed.1
            );
            assert_eq!(read(&alias, 0, expected.len()), expected);
        }
        let observed = f.native.observations.lock().unwrap();
        assert_eq!(
            observed
                .operations
                .iter()
                .filter(|op| matches!(op, Operation::HistoryCommand(HistoryCommand::Commit(_))))
                .count(),
            3
        );
        assert!(!observed.operations.iter().any(|op| matches!(
            op,
            Operation::HistoryCommand(HistoryCommand::StageChanges(_))
        )));
        drop(observed);
        drop(file);
        drop(alias);
        finish(&f, data, &mut mount);
        check("mounted-overwrite-gap-alias-and-A-B-A-incremental-Commits");
    }

    #[test]
    #[ignore = "requires actual kernel append serialization and fd offsets"]
    fn kernel_write_append() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, Some(8));
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let mut first = open_mounted(&f, "data.bin", true);
        let mut second = open_mounted(&f, "alias", true);
        let barrier = Barrier::new(3);
        let positions = std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                barrier.wait();
                first.write_all(b"X").unwrap();
                first.stream_position().unwrap()
            });
            let b = scope.spawn(|| {
                barrier.wait();
                second.write_all(b"Y").unwrap();
                second.stream_position().unwrap()
            });
            barrier.wait();
            (a.join().unwrap(), b.join().unwrap())
        });
        assert!(
            positions == (9, 10) || positions == (10, 9),
            "{positions:?}"
        );
        let tail = read(&first, 8, 2);
        assert!(tail == b"XY" || tail == b"YX");
        let flags = unsafe { libc::fcntl(first.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(first.as_raw_fd(), libc::F_SETFL, flags & !libc::O_APPEND) },
            0
        );
        first.write_all_at(b"P", 0).unwrap();
        assert_eq!(read(&second, 0, 1), b"P");
        assert_eq!(first.metadata().unwrap().len(), 10);
        assert_eq!(
            unsafe { libc::fcntl(first.as_raw_fd(), libc::F_SETFL, flags) },
            0
        );
        let mut toggled = open_mounted(&f, "alias", false);
        let flags = unsafe { libc::fcntl(toggled.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(toggled.as_raw_fd(), libc::F_SETFL, flags | libc::O_APPEND) },
            0
        );
        toggled.write_all(b"F").unwrap();
        assert_eq!(toggled.stream_position().unwrap(), 11);
        assert_eq!(read(&second, 10, 1), b"F");
        drop(toggled);
        // A later SDK length change must be refreshed by the kernel or refused,
        // never silently written at another offset with an incorrect fd position.
        quiescent(&f);
        f.workspace.set_len(data.serial, 15, deadline()).unwrap();
        let result = first.write(b"Z");
        match result {
            Ok(1) => {
                assert_eq!(first.stream_position().unwrap(), 16);
                assert_eq!(read(&second, 15, 1), b"Z");
            }
            Err(error) => {
                assert_eq!(error.raw_os_error(), Some(libc::EINVAL));
                assert_eq!(first.metadata().unwrap().len(), 15);
            }
            other => panic!("{other:?}"),
        }
        assert!(Command::new("sh")
            .arg("-c")
            .arg("printf S >> \"$1\"")
            .arg("sh")
            .arg(f.workspace.mount_path().join("data.bin"))
            .status()
            .unwrap()
            .success());
        let bytes = read(&first, 0, first.metadata().unwrap().len() as usize);
        assert_eq!(*bytes.last().unwrap(), b'S');
        let (_, committed) = saved(&f);
        assert_eq!(f.native.bytes(committed.1, 0, bytes.len()), bytes);
        drop(first);
        drop(second);
        finish(&f, data, &mut mount);
        check("mounted-concurrent-append-preserves-live-EOF-and-fd-positions");
    }

    #[test]
    #[ignore = "requires native READ held while another inode sends actual WRITE"]
    fn kernel_write_read_race() {
        let f = Fixture::new(Gate::ReadHold);
        let data = data(&f, Some(8192));
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let writer = open_mounted(&f, "data.bin", false);
        let other = File::open(f.workspace.mount_path().join("other.bin")).unwrap();
        let before = f.workspace.status().unwrap();
        let backing = f.workspace.backing_status().unwrap();
        std::thread::scope(|scope| {
            let reading = scope.spawn(|| read(&other, 0, 4096));
            f.native.wait_read();
            assert_eq!(
                writer.write_at(b"refused", 0).unwrap_err().raw_os_error(),
                Some(libc::EBUSY)
            );
            assert_eq!(f.workspace.status().unwrap().revision, before.revision);
            assert_eq!(
                f.workspace.backing_status().unwrap().allocated_bytes,
                backing.allocated_bytes
            );
            f.native.release_read();
            assert_eq!(
                reading.join().unwrap(),
                (0..4096).map(|i| (i % 251) as u8).collect::<Vec<_>>()
            );
        });
        writer.write_all_at(b"new", 0).unwrap();
        assert_eq!(read(&writer, 0, 3), b"new");
        saved(&f);
        drop(writer);
        drop(other);
        finish(&f, data, &mut mount);
        check("mounted-old-read-refuses-new-WRITE-before-ingress-and-keeps-read-progress");
    }

    struct Mapping(*mut libc::c_void, usize);
    impl Mapping {
        fn private(file: &File, length: usize) -> Self {
            let address = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    length,
                    libc::PROT_READ,
                    libc::MAP_PRIVATE,
                    file.as_raw_fd(),
                    0,
                )
            };
            assert_ne!(
                address,
                libc::MAP_FAILED,
                "{}",
                std::io::Error::last_os_error()
            );
            Self(address, length)
        }
        fn bytes(&self) -> Vec<u8> {
            (0..self.1)
                .map(|offset| unsafe { self.0.cast::<u8>().add(offset).read_volatile() })
                .collect()
        }
    }
    impl Drop for Mapping {
        fn drop(&mut self) {
            assert_eq!(unsafe { libc::munmap(self.0, self.1) }, 0);
        }
    }
    fn sendfile(f: &Fixture, file: &File, length: usize, name: &str) -> Vec<u8> {
        let path = f
            .workspace
            .mount_path()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(name);
        let out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let mut offset = 0;
        assert_eq!(
            unsafe { libc::sendfile(out.as_raw_fd(), file.as_raw_fd(), &mut offset, length) },
            length as isize
        );
        drop(out);
        let bytes = fs::read(&path).unwrap();
        fs::remove_file(path).unwrap();
        bytes
    }
    #[test]
    #[ignore = "requires actual direct-profile mmap and splice paths"]
    fn kernel_write_mappings() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, Some(8192));
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = open_mounted(&f, "data.bin", false);
        let mapped = Mapping::private(&file, 4096);
        assert_eq!(
            mapped.bytes(),
            (0..4096).map(|i| (i % 251) as u8).collect::<Vec<_>>()
        );
        let shared = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        assert_eq!(shared, libc::MAP_FAILED);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ENODEV)
        );
        assert_eq!(sendfile(&f, &file, 4096, "send-before"), mapped.bytes());
        file.write_all_at(&vec![0x5a; 4096], 0).unwrap();
        assert_eq!(mapped.bytes(), vec![0x5a; 4096]);
        assert_eq!(sendfile(&f, &file, 4096, "send-after"), vec![0x5a; 4096]);
        assert_eq!(
            file.sync_all().unwrap_err().raw_os_error(),
            Some(libc::EOPNOTSUPP)
        );
        for flag in [libc::O_SYNC, libc::O_DSYNC, libc::O_DIRECT] {
            assert_eq!(
                OpenOptions::new()
                    .write(true)
                    .custom_flags(flag)
                    .open(f.workspace.mount_path().join("data.bin"))
                    .unwrap_err()
                    .raw_os_error(),
                Some(libc::EOPNOTSUPP)
            );
        }
        assert_eq!(file.metadata().unwrap().len(), 8192);
        saved(&f);
        drop(mapped);
        drop(file);
        finish(&f, data, &mut mount);
        check("direct-profile-refuses-shared-mmap-and-refreshes-private-mapping-and-sendfile");
    }

    #[test]
    #[ignore = "requires Linux executable direct-I/O mapping route"]
    fn kernel_write_exec() {
        let f = Fixture::new(Gate::None);
        let yes = fs::read("/usr/bin/true").unwrap();
        let no = fs::read("/usr/bin/false").unwrap();
        let data = data(&f, Some(yes.len().max(no.len()) as u64));
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let path = f.workspace.mount_path().join("data.bin");
        for (bytes, exit) in [(&yes, 0), (&no, 1), (&yes, 0)] {
            let mut writer = OpenOptions::new().write(true).open(&path).unwrap();
            writer.write_all(bytes).unwrap();
            drop(writer);
            assert_eq!(Command::new(&path).status().unwrap().code(), Some(exit));
        }
        saved(&f);
        finish(&f, data, &mut mount);
        check("direct-profile-executes-ELF-replaced-through-kernel-WRITE");
    }

    #[test]
    #[ignore = "requires actual C2 RESERVED-lock observer and Linux WRITE"]
    fn kernel_write_native_save() {
        let f = Fixture::new(Gate::NativeSave);
        let data = data(&f, Some(4 * 1024 * 1024));
        let mut bytes = vec![0; 4 * 1024 * 1024];
        let mut random = 0x6a09e667f3bcc909u64;
        for byte in &mut bytes {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            *byte = random as u8;
        }
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let mut file = open_mounted(&f, "data.bin", false);
        file.write_all(&bytes).unwrap();
        assert_eq!(read(&file, 0, bytes.len()), bytes);
        quiescent(&f);
        let ws = f.workspace.clone();
        let commit = std::thread::spawn(move || ws.commit(deadline()));
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "SAVE_PAUSED");
        file.write_all_at(b"LIVE", 0).unwrap();
        assert_eq!(read(&file, 0, 4), b"LIVE");
        println!("STAGE_LIVE_READY");
        std::io::stdout().flush().unwrap();
        let report = commit.join().unwrap().unwrap();
        let root = match report.outcome {
            CommitOutcomeWire::Committed(c) => c.root,
            other => panic!("{other:?}"),
        };
        let old = attr(f.native.attributes(root, b"data.bin"));
        assert_eq!(
            f.native.bytes(old.1, 0, MAX_READ_BYTES),
            bytes[..MAX_READ_BYTES]
        );
        let (_, new) = saved(&f);
        assert_eq!(f.native.bytes(new.1, 0, 4), b"LIVE");
        assert_eq!(read(&file, 0, 4), b"LIVE");
        drop(file);
        finish(&f, data, &mut mount);
        check("mounted-WRITE-and-read-progress-during-actual-C2-save");
    }

    #[test]
    #[ignore = "requires large immutable base and real FUSE ingress"]
    fn kernel_write_ingress() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, None);
        assert_eq!(data.size, 64 * 1024 * 1024);
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = open_mounted(&f, "data.bin", false);
        let before = f
            .native
            .observations
            .lock()
            .unwrap()
            .operations
            .iter()
            .filter(|op| matches!(op, Operation::ReadFile { .. }))
            .count();
        let expected: Vec<_> = (0..MAX_READ_BYTES)
            .map(|i| ((i * 23) % 253) as u8)
            .collect();
        let mut input = expected.clone();
        file.write_all_at(&input, 16 * 1024 * 1024 + 7).unwrap();
        input.fill(0);
        let after = f
            .native
            .observations
            .lock()
            .unwrap()
            .operations
            .iter()
            .filter(|op| matches!(op, Operation::ReadFile { .. }))
            .count();
        assert_eq!(before, after);
        let backing = f.workspace.backing_status().unwrap();
        let payload_bytes =
            backing.allocated_bytes - f.workspace.metadata_status().unwrap().allocated_bytes;
        // Kernel request splitting depends on user-buffer page alignment. Each
        // owned ingress pays its 4 KiB header and at most 4095 padding bytes.
        assert!(backing.payloads > 0);
        assert!(payload_bytes >= MAX_READ_BYTES as u64 + 4096 * backing.payloads as u64);
        assert!(payload_bytes <= MAX_READ_BYTES as u64 + 8191 * backing.payloads as u64);
        assert!(payload_bytes < 2 * MAX_READ_BYTES as u64);
        println!("INGRESS_ALLOCATION {backing:?} payload_bytes={payload_bytes}");
        assert_eq!(file.metadata().unwrap().len(), data.size);
        assert_eq!(read(&file, 16 * 1024 * 1024 + 7, MAX_READ_BYTES), expected);
        let (_, saved) = saved(&f);
        assert_eq!(
            f.native
                .bytes(saved.1, 16 * 1024 * 1024 + 7, MAX_READ_BYTES),
            expected
        );
        drop(file);
        finish(&f, data, &mut mount);
        check("mounted-input-is-owned-before-return-without-large-base-copy-up");
    }

    #[test]
    #[ignore = "requires actual mounted private-disk admission refusal"]
    fn kernel_write_quota() {
        let f = Fixture::with_quota(Gate::None, 1024 * 1024);
        let data = data(&f, None);
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = open_mounted(&f, "data.bin", false);
        let before = f.workspace.status().unwrap();
        assert_eq!(
            file.write_at(b"refused", 0).unwrap_err().raw_os_error(),
            Some(libc::ENOSPC)
        );
        assert_eq!(f.workspace.status().unwrap().revision, before.revision);
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), data);
        assert_eq!(read(&file, 0, 4), [0, 1, 2, 3]);
        drop(file);
        finish(&f, data, &mut mount);
        check("mounted-quota-refusal-preserves-bytes-size-mtime-and-cleanup");
    }

    struct RestoreLimit;
    impl Drop for RestoreLimit {
        fn drop(&mut self) {
            file_limit("unlimited");
        }
    }
    #[test]
    #[ignore = "requires native backing allocation error during FUSE WRITE"]
    fn kernel_write_backing_failure() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, None);
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let file = open_mounted(&f, "data.bin", false);
        file.write_all_at(b"kept", 0).unwrap();
        let before = f.workspace.getattr(data.serial).unwrap();
        let restore = RestoreLimit;
        file_limit("2048");
        let error = file.write_at(b"refused", 0).unwrap_err();
        drop(restore);
        assert_eq!(error.raw_os_error(), Some(libc::EIO));
        assert_eq!(f.workspace.getattr(data.serial).unwrap(), before);
        assert_eq!(read(&file, 0, 4), b"kept");
        let backing = f.workspace.backing_status().unwrap();
        println!("EXPECTED_BACKING_FAILURE {error:?} {backing:?}");
        assert!(backing.allocated_bytes > 0 || backing.reserved_bytes > 0);
        assert!(backing.failed_payloads > 0);
        drop(file);
        mount.unmount(deadline()).unwrap();
        observe(&f);
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 1);
        assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
        check("mounted-backing-failure-retains-accepted-prefix-and-accounted-failed-owner");
    }

    #[test]
    #[ignore = "native public projection API subset; no kernel mount"]
    fn kernel_write_origin() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, Some(8));
        let h = local(&f, data, ReferenceScope::Local, false);
        let input = f.own(b"X");
        let empty = f.own(b"");
        let before_mount = f.workspace.status().unwrap().accounted_bytes;
        let mut lease = f.workspace.reserve_mount().unwrap();
        println!(
            "PROJECTION_LAYOUT observer={} writer={} binding_charge={}",
            std::mem::size_of::<ProjectionReplyPermit>(),
            std::mem::size_of::<ProjectionMutationPermit>(),
            f.workspace.status().unwrap().accounted_bytes - before_mount
        );
        assert!(matches!(
            f.workspace.begin_projection_mutation(deadline()),
            Err(WorkspaceError::Busy)
        ));
        let notifications = Arc::new(AtomicUsize::new(0));
        let counts = notifications.clone();
        lease
            .bind_invalidation(Arc::new(move |_, _, _| {
                counts.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
            .unwrap();
        let projected = local(&f, data, ReferenceScope::Projection, true);
        let observe = f.workspace.begin_projection_reply(deadline()).unwrap();
        assert!(matches!(
            f.workspace.begin_projection_mutation(deadline()),
            Err(WorkspaceError::Busy)
        ));
        drop(observe);
        let before = f.workspace.status().unwrap().revision;
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        assert!(matches!(
            f.workspace.write_file(h, 0, &input, deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert!(permit
            .write_file(projected, 7, &input, true, deadline())
            .is_err());
        assert_eq!(f.workspace.status().unwrap().revision, before);
        assert!(permit
            .write_file(projected, 8, &input, true, deadline())
            .is_err());
        drop(permit);
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        assert!(permit
            .write_file(projected, 7, &empty, true, deadline())
            .is_err());
        drop(permit);
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        let old_reply = f.workspace.begin_projection_reply(deadline()).unwrap();
        assert!(matches!(
            permit.write_file(projected, 8, &input, true, deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(f.workspace.status().unwrap().revision, before);
        drop(old_reply);
        drop(permit);
        assert!(matches!(
            f.workspace
                .begin_projection_mutation(Instant::now() - Duration::from_secs(1)),
            Err(WorkspaceError::Deadline)
        ));
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        assert_eq!(
            permit
                .write_file(projected, 8, &input, true, deadline())
                .unwrap()
                .accepted_bytes,
            1
        );
        assert_eq!(f.workspace.status().unwrap().projection_replies, 1);
        assert_eq!(f.read(h, 8, 1), b"X");
        assert!(matches!(
            f.workspace.write_file(h, 0, &input, deadline()),
            Err(WorkspaceError::Busy)
        ));
        let observe = f.workspace.begin_projection_reply(deadline()).unwrap();
        assert!(matches!(
            f.workspace.begin_projection_reply(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert!(matches!(
            f.workspace.begin_projection_mutation(deadline()),
            Err(WorkspaceError::Busy)
        ));
        drop(observe);
        drop(permit);
        f.workspace.write_file(h, 0, &input, deadline()).unwrap();
        assert_eq!(notifications.load(Ordering::SeqCst), 2);
        f.workspace.release(projected).unwrap();
        lease.finish().unwrap();
        saved(&f);
        drop(input);
        drop(empty);
        f.workspace.release(h).unwrap();
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
        check("projection-write-origin-reply-slot-append-offset-and-single-attempt-contract");
    }

    #[test]
    #[ignore = "native public completion API subset; actual notifier errno covered separately"]
    fn kernel_write_completion_failure() {
        let f = Fixture::new(Gate::None);
        let data = data(&f, Some(8));
        let h = local(&f, data, ReferenceScope::Local, false);
        let alias = f.lookup(b"alias");
        let alias_h = local(&f, alias, ReferenceScope::Local, false);
        let other = f.lookup(b"other.bin");
        let other_h = local(&f, other, ReferenceScope::Local, false);
        let input = f.own(b"known");
        let mut lease = f.workspace.reserve_mount().unwrap();
        lease
            .bind_invalidation(Arc::new(|_, _, _| {
                Err(std::io::Error::from_raw_os_error(libc::EIO))
            }))
            .unwrap();
        let projected = local(&f, data, ReferenceScope::Projection, false);
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        let error = permit
            .write_file(projected, 0, &input, false, deadline())
            .unwrap_err();
        let WorkspaceError::Coherence(failure) = error else {
            panic!("{error:?}")
        };
        assert_eq!(
            (failure.raw_os_error, failure.receipt.accepted_bytes),
            (Some(libc::EIO), 5)
        );
        assert_eq!(f.read(h, 0, 5), b"known");
        assert_eq!(f.workspace.flush(alias_h), Err(error));
        assert_eq!(f.workspace.flush(other_h), Ok(()));
        assert!(matches!(
            f.workspace.begin_projection_mutation(deadline()),
            Err(WorkspaceError::Busy)
        ));
        drop(permit);
        f.workspace.release(projected).unwrap();
        lease.finish().unwrap();
        assert_eq!(f.workspace.flush(h), Ok(()));
        let (_, committed) = saved(&f);
        assert_eq!(f.native.bytes(committed.1, 0, 5), b"known");
        drop(input);
        for handle in [h, alias_h, other_h] {
            f.workspace.release(handle).unwrap();
        }
        for attr in [data, alias, other] {
            f.workspace
                .forget(attr.serial, u64::MAX, ReferenceScope::Local);
        }
        f.workspace.close_clean().unwrap();
        check("projection-write-known-publication-failure-is-reported-by-alias-flush");
    }
}
