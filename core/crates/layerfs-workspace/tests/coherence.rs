//! Bounded projection replies and SDK invalidation, including actual Linux FUSE routes.
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::*;
    use layerfs_workspace::*;
    use std::{
        fs::{self, File, OpenOptions},
        io::{Read, Write},
        os::unix::fs::{FileExt, MetadataExt},
        process::Command,
        sync::{Arc, Condvar, Mutex},
        time::{Duration, Instant},
    };
    fn check(id: &str) {
        println!("COHERENCE_CHECK {id} PASS");
    }
    fn local(f: &Fixture) -> (NodeAttributes, HandleId) {
        let data = f.lookup(b"data.bin");
        let handle = f
            .workspace
            .open_file(
                data.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    append: false,
                    truncate: false,
                },
                ReferenceScope::Local,
                deadline(),
            )
            .unwrap();
        (data, handle)
    }
    fn mounted(f: &Fixture) -> bool {
        fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .any(|line| {
                line.split(' ').nth(4) == Some(f.workspace.mount_path().to_str().unwrap())
                    && (line.contains(" - fuse layerfs ")
                        || line.contains(" - fuse.layerfs layerfs "))
            })
    }
    fn read(file: &File, length: usize) -> Vec<u8> {
        let mut bytes = vec![0; length];
        file.read_exact_at(&mut bytes, 0).unwrap();
        bytes
    }
    fn write(f: &Fixture, handle: HandleId, offset: u64, bytes: &[u8]) -> MutationReceipt {
        let input = f.own(bytes);
        f.workspace
            .write_file(handle, offset, &input, deadline())
            .unwrap()
    }
    fn quiescent(f: &Fixture) {
        let end = deadline();
        while f.workspace.status().unwrap().projection_replies != 0 {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn observe(f: &Fixture) {
        println!(
            "COMMIT_RESOURCE {:?} {:?} {:?}",
            f.workspace.status().unwrap(),
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
    }
    fn close(f: &Fixture, data: NodeAttributes, handle: HandleId) {
        f.workspace.release(handle).unwrap();
        f.workspace
            .forget(data.serial, u64::MAX, ReferenceScope::Local);
        f.workspace.close_clean().unwrap();
    }

    #[test]
    #[ignore = "requires actual privileged Linux FUSE and native service"]
    fn coherence_visibility() {
        let f = Fixture::new(Gate::None);
        let (data, h) = local(&f);
        f.workspace.set_len(data.serial, 8192, deadline()).unwrap();
        let mut mount = layerfs_fuse::mount(&f.workspace, deadline()).unwrap();
        assert!(mounted(&f));
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Ready)
        );
        let file = File::open(f.workspace.mount_path().join("data.bin")).unwrap();
        let alias = File::open(f.workspace.mount_path().join("alias")).unwrap();
        assert_eq!(
            file.metadata().unwrap().ino(),
            alias.metadata().unwrap().ino()
        );
        let mut expected: Vec<u8> = (0..8192).map(|i| (i % 251) as u8).collect();
        assert_eq!(read(&file, 8192), expected);
        assert_eq!(read(&alias, 8192), expected);
        quiescent(&f);
        write(&f, h, 4093, b"ABCDEFG");
        expected[4093..4100].copy_from_slice(b"ABCDEFG");
        assert_eq!(read(&file, 8192), expected);
        assert_eq!(read(&alias, 8192), expected);
        let attr = f.workspace.getattr(data.serial).unwrap();
        let meta = file.metadata().unwrap();
        assert_eq!(
            (meta.mtime(), meta.mtime_nsec()),
            (attr.mtime_seconds, i64::from(attr.mtime_nanoseconds))
        );
        quiescent(&f);
        f.workspace.set_len(data.serial, 4096, deadline()).unwrap();
        expected.truncate(4096);
        assert_eq!(file.metadata().unwrap().len(), 4096);
        assert_eq!(file.read_at(&mut [0; 1], 4096).unwrap(), 0);
        quiescent(&f);
        f.workspace.set_len(data.serial, 8200, deadline()).unwrap();
        expected.resize(8200, 0);
        assert_eq!(read(&file, 8200), expected);
        assert_eq!(read(&alias, 8200), expected);
        let names: Vec<_> = fs::read_dir(f.workspace.mount_path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(names.iter().any(|n| n == "data.bin") && names.iter().any(|n| n == "alias"));
        assert_eq!(
            OpenOptions::new()
                .write(true)
                .open(f.workspace.mount_path().join("data.bin"))
                .unwrap_err()
                .raw_os_error(),
            Some(30)
        );
        f.workspace.commit(deadline()).unwrap();
        quiescent(&f);
        write(&f, h, 0, b"next");
        expected[..4].copy_from_slice(b"next");
        assert_eq!(read(&file, 8200), expected);
        f.workspace.commit(deadline()).unwrap();
        drop(file);
        drop(alias);
        mount.unmount(deadline()).unwrap();
        assert!(!mounted(&f));
        assert_eq!(f.workspace.status().unwrap().coherence, None);
        observe(&f);
        close(&f, data, h);
        check("same-mounted-FDs-and-aliases-see-SDK-bytes-size-mtime-and-repeated-Commit");
    }

    #[test]
    #[ignore = "requires actual mounted old read held at its native delivery"]
    fn coherence_read_race() {
        let f = Fixture::new(Gate::ReadHold);
        let (data, h) = local(&f);
        f.workspace.set_len(data.serial, 8192, deadline()).unwrap();
        let input = f.own(b"NEW!");
        let mut mount = layerfs_fuse::mount(&f.workspace, deadline()).unwrap();
        assert!(mounted(&f));
        let first = f.workspace.begin_projection_reply(deadline()).unwrap();
        let second = f.workspace.begin_projection_reply(deadline()).unwrap();
        assert!(matches!(
            f.workspace.begin_projection_reply(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(f.workspace.status().unwrap().projection_replies, 2);
        assert!(matches!(
            f.workspace.write_file(h, 0, &input, deadline()),
            Err(WorkspaceError::Busy)
        ));
        drop(first);
        drop(second);
        let file = File::open(f.workspace.mount_path().join("data.bin")).unwrap();
        let before = f.workspace.status().unwrap();
        std::thread::scope(|scope| {
            let old = scope.spawn(|| read(&file, 8192));
            f.native.wait_read();
            assert!(f.workspace.status().unwrap().projection_replies > 0);
            assert!(matches!(
                f.workspace.write_file(h, 0, &input, deadline()),
                Err(WorkspaceError::Busy)
            ));
            assert_eq!(f.workspace.status().unwrap().revision, before.revision);
            f.native.release_read();
            assert_eq!(
                old.join().unwrap(),
                (0..8192).map(|i| (i % 251) as u8).collect::<Vec<_>>()
            );
        });
        quiescent(&f);
        f.workspace.write_file(h, 0, &input, deadline()).unwrap();
        assert_eq!(&read(&file, 8192)[..4], b"NEW!");
        drop(input);
        f.workspace.commit(deadline()).unwrap();
        drop(file);
        mount.unmount(deadline()).unwrap();
        assert!(!mounted(&f));
        observe(&f);
        close(&f, data, h);
        check("actual-old-kernel-read-excludes-publication-and-fresh-read-observes-acknowledged-write");
    }

    #[test]
    #[ignore = "requires executable-mode fixture and actual Linux ELF/script execution"]
    fn coherence_exec() {
        let f = Fixture::new(Gate::None);
        let (data, h) = local(&f);
        assert_eq!(data.mode, 0o755);
        let elf = fs::read("/usr/bin/true").unwrap();
        write(&f, h, 0, &elf);
        f.workspace
            .set_len(data.serial, elf.len() as u64, deadline())
            .unwrap();
        let mut mount = layerfs_fuse::mount(&f.workspace, deadline()).unwrap();
        let path = f.workspace.mount_path().join("data.bin");
        assert_eq!(Command::new(&path).status().unwrap().code(), Some(0));
        quiescent(&f);
        let script = b"#!/bin/sh\nexit 7\n";
        write(&f, h, 0, script);
        f.workspace
            .set_len(data.serial, script.len() as u64, deadline())
            .unwrap();
        assert_eq!(Command::new(&path).status().unwrap().code(), Some(7));
        f.workspace.commit(deadline()).unwrap();
        mount.unmount(deadline()).unwrap();
        assert!(!mounted(&f));
        observe(&f);
        close(&f, data, h);
        check("mounted-ELF-and-replacement-script-execute-after-checked-SDK-invalidation");
    }

    #[test]
    #[ignore = "requires actual C2-save observer and mounted SDK mutation progress"]
    fn coherence_native_save() {
        let f = Fixture::new(Gate::NativeSave);
        let (data, h) = local(&f);
        let mut bytes = vec![0; 4 * 1024 * 1024];
        let mut random = 0x6a09e667f3bcc909u64;
        for b in &mut bytes {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            *b = random as u8;
        }
        write(&f, h, 0, &bytes);
        f.workspace
            .set_len(data.serial, bytes.len() as u64, deadline())
            .unwrap();
        let mut mount = layerfs_fuse::mount(&f.workspace, deadline()).unwrap();
        let file = File::open(f.workspace.mount_path().join("data.bin")).unwrap();
        assert_eq!(read(&file, bytes.len()), bytes);
        quiescent(&f);
        let ws = f.workspace.clone();
        let save = std::thread::spawn(move || ws.commit(deadline()));
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "SAVE_PAUSED");
        write(&f, h, 0, b"LIVE");
        assert_eq!(&read(&file, 4096)[..4], b"LIVE");
        println!("STAGE_LIVE_READY");
        std::io::stdout().flush().unwrap();
        let first = save.join().unwrap().unwrap();
        let root = match first.outcome {
            CommitOutcomeWire::Committed(c) => c.root,
            _ => panic!("dirty Commit"),
        };
        let content = attr(f.native.attributes(root, b"data.bin"));
        assert_eq!(
            f.native.bytes(content.1, 0, MAX_READ_BYTES),
            bytes[..MAX_READ_BYTES]
        );
        f.workspace.commit(deadline()).unwrap();
        assert_eq!(&read(&file, 4096)[..4], b"LIVE");
        drop(file);
        mount.unmount(deadline()).unwrap();
        assert!(!mounted(&f));
        observe(&f);
        close(&f, data, h);
        check("mounted-read-sees-live-SDK-write-while-actual-C2-save-retains-G");
    }

    #[derive(Default)]
    struct CompletionGate {
        state: Mutex<(bool, bool)>,
        changed: Condvar,
    }
    impl CompletionGate {
        fn delivery(self: &Arc<Self>) -> ProjectionInvalidation {
            let gate = self.clone();
            Arc::new(move |_, _, _| {
                let mut s = gate.state.lock().unwrap();
                s.0 = true;
                gate.changed.notify_all();
                let (_state, timed) = gate
                    .changed
                    .wait_timeout_while(s, Duration::from_secs(5), |s| !s.1)
                    .unwrap();
                assert!(!timed.timed_out());
                Ok(())
            })
        }
        fn wait(&self) {
            let s = self.state.lock().unwrap();
            let (_state, timed) = self
                .changed
                .wait_timeout_while(s, Duration::from_secs(5), |s| !s.0)
                .unwrap();
            assert!(!timed.timed_out());
        }
        fn release(&self) {
            self.state.lock().unwrap().1 = true;
            self.changed.notify_all();
        }
    }
    #[test]
    #[ignore = "requires native service; projection-completion API subset without a kernel mount"]
    fn coherence_completion() {
        let f = Fixture::new(Gate::None);
        let (data, h) = local(&f);
        let input = f.own(b"NEW!");
        let gate = Arc::new(CompletionGate::default());
        let mut lease = f.workspace.reserve_mount().unwrap();
        assert!(matches!(
            f.workspace.write_file(h, 0, &input, deadline()),
            Err(WorkspaceError::Busy)
        ));
        lease.bind_invalidation(gate.delivery()).unwrap();
        std::thread::scope(|scope| {
            let writing = scope.spawn(|| f.workspace.write_file(h, 0, &input, deadline()));
            gate.wait();
            let pending = f.workspace.status().unwrap();
            assert!(matches!(
                pending.coherence,
                Some(CoherenceStatus::Pending { .. })
            ));
            let fresh = f.workspace.begin_projection_reply(deadline()).unwrap();
            assert_eq!(f.read(h, 0, 4), b"NEW!");
            drop(fresh);
            assert!(matches!(
                f.workspace.write_file(h, 4, &input, deadline()),
                Err(WorkspaceError::Busy)
            ));
            assert_eq!(lease.finish(), Err(WorkspaceError::Busy));
            f.workspace.commit(deadline()).unwrap();
            assert!(f.workspace.status().unwrap().generation > pending.generation);
            gate.release();
            let receipt = writing.join().unwrap().unwrap();
            assert_eq!(receipt.generation, pending.generation);
        });
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Ready)
        );
        lease.finish().unwrap();
        drop(input);
        observe(&f);
        close(&f, data, h);
        check("pending-notification-allows-new-view-and-Commit-but-excludes-next-publication");
    }
    #[test]
    #[ignore = "requires actual deadline expiry in the public completion binding; no kernel timing claim"]
    fn coherence_deadline() {
        let f = Fixture::new(Gate::None);
        let (data, h) = local(&f);
        let input = f.own(b"NEW!");
        let gate = Arc::new(CompletionGate::default());
        let mut lease = f.workspace.reserve_mount().unwrap();
        lease.bind_invalidation(gate.delivery()).unwrap();
        let end = Instant::now() + Duration::from_millis(200);
        let error = std::thread::scope(|scope| {
            let writing = scope.spawn(|| f.workspace.write_file(h, 0, &input, end));
            gate.wait();
            while Instant::now() <= end {
                std::thread::sleep(Duration::from_millis(1));
            }
            gate.release();
            writing.join().unwrap().unwrap_err()
        });
        let WorkspaceError::Coherence(failure) = error else {
            panic!("published result lost: {error:?}")
        };
        assert!(failure.notifier_returned_ok);
        assert_eq!(failure.kind, std::io::ErrorKind::TimedOut);
        assert_eq!(failure.receipt.accepted_bytes, 4);
        assert_eq!(f.read(h, 0, 4), b"NEW!");
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Failed(failure))
        );
        assert!(matches!(
            f.workspace.write_file(h, 4, &input, deadline()),
            Err(WorkspaceError::Busy)
        ));
        lease.finish().unwrap();
        f.workspace.commit(deadline()).unwrap();
        drop(input);
        observe(&f);
        close(&f, data, h);
        check("post-publication-deadline-preserves-receipt-and-retained-completed-failure");
    }

    fn filter_count(tid: i32) -> u32 {
        fs::read_to_string(format!("/proc/self/task/{tid}/status"))
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("Seccomp_filters:"))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }
    fn fuse_fd() -> i32 {
        use std::os::unix::fs::FileTypeExt;
        let entries: Vec<_> = fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.unwrap();
                (fs::read_link(entry.path()).ok().as_deref()
                    == Some(std::path::Path::new("/dev/fuse")))
                .then(|| entry.file_name().to_str().unwrap().parse::<i32>().unwrap())
            })
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "fault requires one shared FUSE device descriptor"
        );
        let actual = fs::metadata(format!("/proc/self/fd/{}", entries[0])).unwrap();
        let device = fs::metadata("/dev/fuse").unwrap();
        assert!(actual.file_type().is_char_device());
        assert_eq!(actual.rdev(), device.rdev());
        entries[0]
    }
    fn deny_notifier_writev(fd: i32) {
        use nix::libc;
        assert!(fd >= 0 && cfg!(target_endian = "little"));
        let arch = if cfg!(target_arch = "aarch64") {
            0xc00000b7
        } else if cfg!(target_arch = "x86_64") {
            0xc000003e
        } else {
            panic!("unsupported fault architecture")
        };
        let stmt = |code, k| libc::sock_filter {
            code,
            jt: 0,
            jf: 0,
            k,
        };
        let jump = |k, jf| libc::sock_filter {
            code: 0x15,
            jt: 0,
            jf,
            k,
        };
        // Native arch, syscall nr, then low32 of args[0]. This is a targeted
        // external test fault, not a security sandbox; all other calls pass.
        let mut code = [
            stmt(0x20, 4),
            jump(arch, 5),
            stmt(0x20, 0),
            jump(libc::SYS_writev as u32, 3),
            stmt(0x20, 16),
            jump(fd as u32, 1),
            stmt(0x06, 0x00050000 | libc::EIO as u32),
            stmt(0x06, 0x7fff0000),
        ];
        let program = libc::sock_fprog {
            len: code.len() as u16,
            filter: code.as_mut_ptr(),
        };
        // Flags zero means this dedicated caller thread only, never TSYNC.
        unsafe {
            assert_eq!(
                libc::prctl(
                    libc::PR_SET_NO_NEW_PRIVS,
                    1 as libc::c_ulong,
                    0 as libc::c_ulong,
                    0 as libc::c_ulong,
                    0 as libc::c_ulong,
                ),
                0,
                "{}",
                std::io::Error::last_os_error()
            );
            assert_eq!(
                libc::syscall(libc::SYS_seccomp, 1, 0, &program),
                0,
                "{}",
                std::io::Error::last_os_error()
            );
        }
    }
    fn notification_fault(truncate: bool) {
        use std::{io::IoSlice, os::unix::net::UnixStream};
        let f = Fixture::new(Gate::None);
        let (data, h) = local(&f);
        f.workspace.set_len(data.serial, 8192, deadline()).unwrap();
        let input = f.own(b"FAIL");
        let mut mount = layerfs_fuse::mount(&f.workspace, deadline()).unwrap();
        let file = File::open(f.workspace.mount_path().join("data.bin")).unwrap();
        assert_eq!(
            read(&file, 8192),
            (0..8192).map(|i| (i % 251) as u8).collect::<Vec<_>>()
        );
        quiescent(&f);
        let fd = fuse_fd();
        let before = f.workspace.status().unwrap();
        let workers: Vec<_> = fs::read_dir("/proc/self/task")
            .unwrap()
            .map(|e| {
                let tid: i32 = e.unwrap().file_name().to_str().unwrap().parse().unwrap();
                (tid, filter_count(tid))
            })
            .collect();
        let calls = f.native.observations.lock().unwrap().operations.len();
        let (mut ordinary, mut consumer) = UnixStream::pair().unwrap();
        let (error, tid, filters_before, filters_after) = std::thread::scope(|scope| {
            let fault = scope.spawn(|| {
                let tid = unsafe { nix::libc::syscall(nix::libc::SYS_gettid) as i32 };
                let filters_before = filter_count(tid);
                deny_notifier_writev(fd);
                let filters_after = filter_count(tid);
                assert_eq!(
                    ordinary
                        .write_vectored(&[IoSlice::new(b"allowed")])
                        .unwrap(),
                    7
                );
                let error = if truncate {
                    f.workspace
                        .open_file(
                            data.serial,
                            FileOpenOptions {
                                access: FileAccess::ReadWrite,
                                append: false,
                                truncate: true,
                            },
                            ReferenceScope::Local,
                            deadline(),
                        )
                        .unwrap_err()
                } else {
                    f.workspace
                        .write_file(h, 0, &input, deadline())
                        .unwrap_err()
                };
                (error, tid, filters_before, filters_after)
            });
            let mut bytes = [0; 7];
            consumer.read_exact(&mut bytes).unwrap();
            assert_eq!(&bytes, b"allowed");
            fault.join().unwrap()
        });
        assert_eq!(filters_after, filters_before + 1);
        // join completed the faulting caller; immediate procfs task removal is
        // a separate kernel teardown event, not a notification guarantee.
        for (tid, filters) in workers {
            assert_eq!(
                filter_count(tid),
                filters,
                "filter spread to an existing thread"
            );
        }
        assert_eq!(
            f.native.observations.lock().unwrap().operations.len(),
            calls,
            "filtered caller opened a remote helper lane"
        );
        let WorkspaceError::Coherence(failure) = error else {
            panic!("published result lost: {error:?}")
        };
        assert_eq!(failure.raw_os_error, Some(nix::libc::EIO));
        assert!(!failure.notifier_returned_ok);
        assert_eq!(failure.receipt.revision, before.revision + 1);
        assert_eq!(failure.receipt.inode, data.serial);
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Failed(failure))
        );
        assert!(mounted(&f));
        let expected_length = if truncate { 0 } else { 8192 };
        assert_eq!(
            file.metadata().unwrap().len(),
            expected_length,
            "unfiltered FUSE replies must still run"
        );
        if truncate {
            let published = failure
                .published_handle
                .expect("READY truncating handle was hidden");
            assert_eq!(failure.receipt.accepted_bytes, 0);
            assert!(f.read(published, 0, 1).is_empty());
            f.workspace.release(published).unwrap();
        } else {
            assert_eq!(failure.published_handle, None);
            assert_eq!(failure.receipt.accepted_bytes, 4);
            assert_eq!(f.read(h, 0, 4), b"FAIL");
        }
        assert!(matches!(
            f.workspace.write_file(h, 0, &input, deadline()),
            Err(WorkspaceError::Busy)
        ));
        println!("COMMIT_FAILURE native-seccomp-notifier-writev fd={fd} tid={tid} filters={filters_before}->{filters_after} {failure:?}");
        drop(ordinary);
        drop(consumer);
        drop(file);
        mount.unmount(deadline()).unwrap();
        assert!(!mounted(&f));
        assert_eq!(f.workspace.status().unwrap().coherence, None);
        assert_eq!(f.workspace.status().unwrap().dirty_inodes, 1);
        let report = f.workspace.commit(deadline()).unwrap();
        let root = match report.outcome {
            CommitOutcomeWire::Committed(c) => c.root,
            _ => panic!("dirty Commit"),
        };
        let saved = attr(f.native.attributes(root, b"data.bin"));
        assert_eq!(saved.2, expected_length);
        if !truncate {
            assert_eq!(f.native.bytes(saved.1, 0, 4), b"FAIL");
        }
        drop(input);
        observe(&f);
        close(&f, data, h);
    }
    #[test]
    #[ignore = "requires native thread-local seccomp denial of real notifier writev"]
    fn coherence_notify_failure() {
        notification_fault(false);
        check("real-notifier-send-failure-preserves-published-write-and-allows-checked-detach");
    }
    #[test]
    #[ignore = "requires native notifier failure after atomic truncating-open publication"]
    fn coherence_truncate_failure() {
        notification_fault(true);
        check("real-notifier-failure-exposes-READY-truncating-handle-and-preserves-empty-file");
    }
}
