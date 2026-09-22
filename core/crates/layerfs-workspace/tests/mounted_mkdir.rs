//! Actual FUSE mkdir and checked SDK entry invalidation; no product fault hooks.
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::*;
    use layerfs_workspace::*;
    use std::{
        fs::{self, File},
        io::{IoSlice, Read, Seek, SeekFrom, Write},
        os::{
            fd::AsRawFd,
            unix::{
                fs::{FileTypeExt, MetadataExt},
                net::UnixStream,
            },
        },
        path::Path,
        process::Command,
        time::{Duration, Instant},
    };

    fn check(name: &str) {
        println!("MOUNTED_MKDIR_CHECK {name} PASS");
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
    fn writable_mount(f: &Fixture) -> layerfs_fuse::MountHandle {
        let mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let mountinfo = fs::read_to_string("/proc/self/mountinfo").unwrap();
        let row = mountinfo
            .lines()
            .find(|line| {
                line.split(' ').nth(4) == Some(f.workspace.mount_path().to_str().unwrap())
                    && (line.contains(" - fuse layerfs ")
                        || line.contains(" - fuse.layerfs layerfs "))
            })
            .expect("actual FUSE mount must exist");
        let options = row.split(' ').nth(5).unwrap();
        assert!(options.split(',').any(|option| option == "rw"), "{row}");
        assert!(!options.split(',').any(|option| option == "ro"), "{row}");
        println!("MKDIR_RESOURCE mount_profile=writable actual_mount_options={options}");
        mount
    }
    fn quiescent(f: &Fixture) {
        let end = deadline();
        while f.workspace.status().unwrap().projection_replies != 0 {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn snapshot(f: &Fixture) -> BranchSnapshotWire {
        let Response::History(value) = f.branch() else {
            panic!("history")
        };
        let HistoryResult::BranchSnapshot(value) = *value else {
            panic!("branch")
        };
        value
    }
    fn count(f: &Fixture, reserves: bool) -> usize {
        f.native
            .observations
            .lock()
            .unwrap()
            .operations
            .iter()
            .filter(|op| {
                if reserves {
                    matches!(
                        op,
                        Operation::HistoryCommand(HistoryCommand::ReserveInodes { .. })
                    )
                } else {
                    matches!(
                        op,
                        Operation::HistoryCommand(
                            HistoryCommand::Commit(_) | HistoryCommand::CommitStaged { .. }
                        )
                    )
                }
            })
            .count()
    }
    fn lookup(f: &Fixture, parent: u64, name: &[u8]) -> NodeAttributes {
        f.workspace
            .lookup(parent, name, ReferenceScope::Local, deadline())
            .unwrap()
    }
    fn commit(f: &Fixture) -> Root {
        let report = f.workspace.commit(deadline()).unwrap();
        assert!(f.workspace.status().unwrap().submission.is_none());
        let CommitOutcomeWire::Committed(commit) = report.outcome else {
            panic!("new commit")
        };
        assert_eq!(snapshot(f).effective_root, commit.root);
        commit.root
    }
    fn saved(f: &Fixture, root: Root, path: &[u8], a: NodeAttributes) {
        let Response::Attributes {
            serial,
            kind,
            references,
            mode,
            mtime,
            nanoseconds,
            size,
            ..
        } = f.native.attributes(root, path)
        else {
            panic!("attributes")
        };
        assert_eq!(
            (serial, kind, references, mode, mtime, nanoseconds, size),
            (
                a.serial,
                2,
                1,
                a.mode,
                a.mtime_seconds,
                a.mtime_nanoseconds,
                0
            )
        );
    }
    fn missing(f: &Fixture, root: Root, name: &[u8]) {
        assert_eq!(
            f.native
                .request(
                    Operation::Inspect {
                        root,
                        query: Inspect::Attributes {
                            path: name.to_vec()
                        }
                    },
                    0,
                    &mut std::io::sink()
                )
                .unwrap_err()
                .code,
            Code::PathNotFound
        );
    }
    fn close(f: &Fixture, local: &[u64]) {
        assert!(!mounted(f));
        for serial in local {
            f.workspace.forget(*serial, 1, ReferenceScope::Local);
            assert_eq!(f.workspace.getattr(*serial), Err(WorkspaceError::NotFound));
        }
        println!(
            "MKDIR_RESOURCE {:?} {:?} {:?}",
            f.workspace.status().unwrap(),
            f.workspace.backing_status().unwrap(),
            f.workspace.metadata_status().unwrap()
        );
        f.workspace.close_clean().unwrap();
        check("native-clean-close");
    }
    fn kernel_metadata(path: &Path, a: NodeAttributes) {
        let metadata = fs::metadata(path).unwrap();
        assert!(metadata.is_dir());
        assert_eq!(
            (
                metadata.ino(),
                metadata.mode() & 0o7777,
                metadata.mtime(),
                metadata.mtime_nsec()
            ),
            (
                a.serial,
                a.mode,
                a.mtime_seconds,
                i64::from(a.mtime_nanoseconds)
            )
        );
    }

    #[test]
    #[ignore = "requires native Service and real privileged Linux FUSE"]
    fn mounted_mkdir_kernel() {
        let f = Fixture::new(Gate::None);
        let before = snapshot(&f);
        let mut mount = writable_mount(&f);
        assert!(mounted(&f));
        let result = Command::new("python3")
            .args([
                "-c",
                r#"
import errno, os, sys
p=sys.argv[1]
os.umask(0o027)
os.mkdir(p+'/kernel',0o1777)
os.mkdir(p+'/kernel/nested',0o777)
assert os.stat(p+'/kernel').st_mode & 0o7777 == 0o1750
assert os.stat(p+'/kernel/nested').st_mode & 0o7777 == 0o750
try: os.mkdir(p+'/kernel',0o777)
except OSError as e: assert e.errno == errno.EEXIST
else: raise AssertionError('duplicate accepted')
try: os.mkdir(p+'/'+'x'*256,0o777)
# Current shared child_path InvalidInput maps to EINVAL; this is not a
# kernel NAME_MAX preflight or a separately qualified POSIX length errno.
except OSError as e: assert e.errno == errno.EINVAL
else: raise AssertionError('long component accepted')
print('KERNEL_MKDIR nested=2 umask=027 duplicate=EEXIST long=EINVAL(native-InvalidInput)')
"#,
            ])
            .arg(f.workspace.mount_path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        print!("{}", String::from_utf8_lossy(&result.stdout));
        quiescent(&f);
        let a = lookup(&f, f.workspace.root().serial, b"kernel");
        let b = lookup(&f, a.serial, b"nested");
        assert_eq!((a.mode, b.mode), (0o1750, 0o750));
        kernel_metadata(&f.workspace.mount_path().join("kernel"), a);
        kernel_metadata(&f.workspace.mount_path().join("kernel/nested"), b);
        assert_eq!(count(&f, true), 2);
        assert_eq!(count(&f, false), 0);
        assert_eq!(snapshot(&f), before);
        let root = commit(&f);
        assert_eq!(count(&f, false), 1);
        saved(&f, root, b"kernel", a);
        saved(&f, root, b"kernel/nested", b);
        missing(&f, before.effective_root, b"kernel");
        mount.unmount(deadline()).unwrap();
        check("actual-kernel-nested-mkdir-mode-umask-EEXIST-and-explicit-Commit");
        close(&f, &[a.serial, b.serial]);
    }

    fn directory_page(file: &File) -> Vec<(Vec<u8>, u64, u64, u8)> {
        let mut bytes = [0u8; 64];
        let count = unsafe {
            nix::libc::syscall(
                nix::libc::SYS_getdents64,
                file.as_raw_fd(),
                bytes.as_mut_ptr(),
                bytes.len(),
            )
        };
        assert!(count >= 0, "{}", std::io::Error::last_os_error());
        let mut rows = Vec::new();
        let mut offset = 0;
        while offset < count as usize {
            assert!(offset + 20 <= count as usize);
            let record = &bytes[offset..count as usize];
            let size = u16::from_ne_bytes(record[16..18].try_into().unwrap()) as usize;
            assert!(size >= 20 && offset + size <= count as usize);
            let name_end = record[19..size].iter().position(|b| *b == 0).unwrap() + 19;
            rows.push((
                record[19..name_end].to_vec(),
                u64::from_ne_bytes(record[..8].try_into().unwrap()),
                u64::from_ne_bytes(record[8..16].try_into().unwrap()),
                record[18],
            ));
            offset += size;
        }
        rows
    }
    fn directory_tail(file: &File) -> Vec<(Vec<u8>, u64, u64, u8)> {
        let mut all = Vec::new();
        loop {
            let page = directory_page(file);
            if page.is_empty() {
                break;
            }
            all.extend(page);
            assert!(all.len() < 16);
        }
        all
    }

    #[test]
    #[ignore = "requires negative kernel lookup and stable actual getdents handle"]
    fn mounted_mkdir_visibility() {
        let f = Fixture::new(Gate::None);
        let before = snapshot(&f);
        let mut mount = writable_mount(&f);
        let path = f.workspace.mount_path();
        assert_eq!(
            fs::metadata(path.join("sdk")).unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        let cached_parent = fs::metadata(path).unwrap();
        let mut old = File::open(path).unwrap();
        let prefix = directory_page(&old);
        assert!(!prefix.is_empty());
        let cookie = prefix.last().unwrap().2;
        let tail = directory_tail(&old);
        assert!(!tail.is_empty());
        quiescent(&f);
        let a = f
            .workspace
            .mkdir(f.workspace.root().serial, b"sdk", 0o750, 0, deadline())
            .unwrap();
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Ready)
        );
        kernel_metadata(&path.join("sdk"), a);
        let parent = f.workspace.getattr(f.workspace.root().serial).unwrap();
        let fresh = fs::metadata(path).unwrap();
        assert_eq!(
            (fresh.mtime(), fresh.mtime_nsec()),
            (parent.mtime_seconds, i64::from(parent.mtime_nanoseconds))
        );
        assert_ne!(
            (fresh.mtime(), fresh.mtime_nsec()),
            (cached_parent.mtime(), cached_parent.mtime_nsec())
        );
        fs::create_dir(path.join("sdk/kernel-child")).unwrap();
        quiescent(&f);
        let b = lookup(&f, a.serial, b"kernel-child");
        kernel_metadata(&path.join("sdk/kernel-child"), b);
        old.seek(SeekFrom::Start(cookie)).unwrap();
        assert_eq!(directory_tail(&old), tail);
        let names: Vec<_> = fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(names.iter().any(|name| name == "sdk"));
        assert!(!prefix.iter().chain(&tail).any(|row| row.0 == b"sdk"));
        assert_eq!(count(&f, true), 2);
        assert_eq!(count(&f, false), 0);
        let root = commit(&f);
        assert_eq!(count(&f, false), 1);
        old.seek(SeekFrom::Start(cookie)).unwrap();
        assert_eq!(directory_tail(&old), tail);
        saved(&f, root, b"sdk/kernel-child", b);
        saved(&f, root, b"sdk", f.workspace.getattr(a.serial).unwrap());
        missing(&f, before.effective_root, b"sdk");
        drop(old);
        mount.unmount(deadline()).unwrap();
        check("mounted-SDK-kernel-visibility-negative-lookup-and-stable-old-directory-handle");
        close(&f, &[a.serial, b.serial]);
    }

    #[test]
    #[ignore = "requires actual mount binding; public projection permit subset"]
    fn mounted_mkdir_permit() {
        let f = Fixture::new(Gate::None);
        let mut mount = writable_mount(&f);
        quiescent(&f);
        let before = f.workspace.status().unwrap();
        let old = f.workspace.begin_projection_reply(deadline()).unwrap();
        assert_eq!(
            f.workspace
                .mkdir(f.workspace.root().serial, b"excluded", 0o755, 0, deadline()),
            Err(WorkspaceError::Busy)
        );
        assert!(matches!(
            f.workspace.begin_projection_mutation(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(count(&f, true), 0);
        assert_eq!(f.workspace.status().unwrap().revision, before.revision);
        drop(old);
        let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
        assert_eq!(
            permit.mkdir(f.workspace.root().serial, b"bad/name", 0o755, 0, deadline()),
            Err(WorkspaceError::InvalidInput)
        );
        assert_eq!(
            permit.mkdir(f.workspace.root().serial, b"used", 0o755, 0, deadline()),
            Err(WorkspaceError::InvalidInput)
        );
        assert_eq!(count(&f, true), 0);
        drop(permit);
        let fd = fuse_fd();
        // This direct projected call must not invoke the SDK notifier. The
        // actual kernel syscall route is independently covered by kernel/visibility.
        let a = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    deny_notifier_writev(fd, None);
                    let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
                    let a = permit
                        .mkdir(
                            f.workspace.root().serial,
                            b"projected",
                            0o755,
                            0,
                            deadline(),
                        )
                        .unwrap();
                    assert_eq!(
                        permit.mkdir(f.workspace.root().serial, b"again", 0o755, 0, deadline()),
                        Err(WorkspaceError::InvalidInput)
                    );
                    a
                })
                .join()
                .unwrap()
        });
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Ready)
        );
        assert_eq!(count(&f, true), 1);
        f.workspace.forget(a.serial, 1, ReferenceScope::Projection);
        assert_eq!(f.workspace.getattr(a.serial), Err(WorkspaceError::NotFound));
        assert_eq!(lookup(&f, f.workspace.root().serial, b"projected"), a);
        let root = commit(&f);
        saved(&f, root, b"projected", a);
        mount.unmount(deadline()).unwrap();
        check("projection-reply-excludes-mkdir-and-permit-is-used-once-without-notification");
        close(&f, &[a.serial]);
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
        let fds: Vec<i32> = fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.unwrap();
                (fs::read_link(entry.path()).ok().as_deref() == Some(Path::new("/dev/fuse")))
                    .then(|| entry.file_name().to_str().unwrap().parse().unwrap())
            })
            .collect();
        assert_eq!(fds.len(), 1);
        let actual = fs::metadata(format!("/proc/self/fd/{}", fds[0])).unwrap();
        assert!(actual.file_type().is_char_device());
        assert_eq!(actual.rdev(), fs::metadata("/dev/fuse").unwrap().rdev());
        fds[0]
    }
    fn deny_notifier_writev(fd: i32, iovcnt: Option<u32>) {
        use nix::libc;
        assert!(fd >= 0 && cfg!(target_endian = "little"));
        let arch = if cfg!(target_arch = "aarch64") {
            0xc00000b7
        } else if cfg!(target_arch = "x86_64") {
            0xc000003e
        } else {
            panic!("fault architecture")
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
        // None denies every writev on this exact /dev/fuse fd. Some(4) denies
        // entry notifications while permitting the two-iovec parent invalidation.
        let vectors = match iovcnt {
            Some(count) => jump(count, 1),
            None => stmt(0x05, 0), // Unconditional fallthrough to EIO.
        };
        let mut code = [
            stmt(0x20, 4),
            jump(arch, 7),
            stmt(0x20, 0),
            jump(libc::SYS_writev as u32, 5),
            stmt(0x20, 16),
            jump(fd as u32, 3),
            stmt(0x20, 32),
            vectors,
            stmt(0x06, 0x00050000 | libc::EIO as u32),
            stmt(0x06, 0x7fff0000),
        ];
        let program = libc::sock_fprog {
            len: code.len() as u16,
            filter: code.as_mut_ptr(),
        };
        unsafe {
            assert_eq!(
                libc::prctl(
                    libc::PR_SET_NO_NEW_PRIVS,
                    1 as libc::c_ulong,
                    0 as libc::c_ulong,
                    0 as libc::c_ulong,
                    0 as libc::c_ulong
                ),
                0
            );
            assert_eq!(libc::syscall(libc::SYS_seccomp, 1, 0, &program), 0);
        }
    }

    #[test]
    #[ignore = "requires thread-local seccomp on actual name-notifier writev"]
    fn mounted_mkdir_notification_failure() {
        let f = Fixture::new(Gate::None);
        let before_branch = snapshot(&f);
        let mut mount = writable_mount(&f);
        let cached = fs::metadata(f.workspace.mount_path()).unwrap();
        assert_eq!(
            fs::metadata(f.workspace.mount_path().join("retained"))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
        quiescent(&f);
        let before = f.workspace.status().unwrap();
        let fd = fuse_fd();
        let workers: Vec<_> = fs::read_dir("/proc/self/task")
            .unwrap()
            .map(|e| {
                let tid: i32 = e.unwrap().file_name().to_str().unwrap().parse().unwrap();
                (tid, filter_count(tid))
            })
            .collect();
        let (mut ordinary, mut reader) = UnixStream::pair().unwrap();
        let (error, tid, old_filters, new_filters) = std::thread::scope(|scope| {
            let child = scope.spawn(|| {
                let tid = unsafe { nix::libc::syscall(nix::libc::SYS_gettid) as i32 };
                let old = filter_count(tid);
                deny_notifier_writev(fd, Some(4));
                let new = filter_count(tid);
                assert_eq!(
                    ordinary
                        .write_vectored(&[
                            IoSlice::new(b"a"),
                            IoSlice::new(b"b"),
                            IoSlice::new(b"c"),
                            IoSlice::new(b"d")
                        ])
                        .unwrap(),
                    4
                );
                let error = f
                    .workspace
                    .mkdir(f.workspace.root().serial, b"retained", 0o750, 0, deadline())
                    .unwrap_err();
                (error, tid, old, new)
            });
            let mut bytes = [0; 4];
            reader.read_exact(&mut bytes).unwrap();
            assert_eq!(&bytes, b"abcd");
            child.join().unwrap()
        });
        assert_eq!(new_filters, old_filters + 1);
        // join completed the faulting caller; immediate procfs task removal is
        // a separate kernel teardown event, not a notification guarantee.
        for (worker, filters) in workers {
            assert_eq!(filter_count(worker), filters);
        }
        let WorkspaceError::Coherence(failure) = error else {
            panic!("{error:?}")
        };
        assert_eq!(failure.raw_os_error, Some(nix::libc::EIO));
        assert!(!failure.notifier_returned_ok);
        assert_eq!(failure.published_handle, None);
        assert_eq!(failure.receipt.accepted_bytes, 0);
        assert_eq!(failure.receipt.revision, before.revision + 1);
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Failed(failure))
        );
        assert_eq!(count(&f, true), 1);
        assert_eq!(count(&f, false), 0);
        assert_eq!(
            f.workspace.getattr(failure.receipt.inode),
            Err(WorkspaceError::NotFound),
            "withheld Local return ref released"
        );
        let a = lookup(&f, f.workspace.root().serial, b"retained");
        assert_eq!(a.serial, failure.receipt.inode);
        f.workspace.forget(a.serial, 1, ReferenceScope::Local);
        assert_eq!(f.workspace.getattr(a.serial), Err(WorkspaceError::NotFound));
        let parent = f.workspace.getattr(f.workspace.root().serial).unwrap();
        let fresh = fs::metadata(f.workspace.mount_path()).unwrap();
        assert_eq!(
            (fresh.mtime(), fresh.mtime_nsec()),
            (parent.mtime_seconds, i64::from(parent.mtime_nanoseconds))
        );
        assert_ne!(
            (fresh.mtime(), fresh.mtime_nsec()),
            (cached.mtime(), cached.mtime_nsec())
        );
        assert_eq!(
            f.workspace
                .mkdir(f.workspace.root().serial, b"retained", 0o750, 0, deadline()),
            Err(WorkspaceError::Busy)
        );
        assert_eq!(count(&f, true), 1);
        assert_eq!(
            f.workspace.status().unwrap().revision,
            failure.receipt.revision
        );
        assert_eq!(snapshot(&f), before_branch);
        println!("MKDIR_RESOURCE notification fd={fd} iovcnt=4 tid={tid} filters={old_filters}->{new_filters} parent_attrs_visible=true {failure:?}");
        drop(ordinary);
        drop(reader);
        mount.unmount(deadline()).unwrap();
        assert!(!mounted(&f));
        assert_eq!(f.workspace.status().unwrap().coherence, None);
        assert!(f.workspace.status().unwrap().dirty_inodes > 0);
        let mut repaired = writable_mount(&f);
        assert!(mounted(&f));
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Ready)
        );
        kernel_metadata(&f.workspace.mount_path().join("retained"), a);
        assert_eq!(lookup(&f, f.workspace.root().serial, b"retained"), a);
        assert_eq!(count(&f, true), 1);
        let root = commit(&f);
        assert_eq!(count(&f, false), 1);
        saved(&f, root, b"retained", a);
        missing(&f, before_branch.effective_root, b"retained");
        repaired.unmount(deadline()).unwrap();
        check("entry-notification-failure-retains-name-no-ref-leak-and-checked-remount-repairs");
        close(&f, &[a.serial]);
    }
}
