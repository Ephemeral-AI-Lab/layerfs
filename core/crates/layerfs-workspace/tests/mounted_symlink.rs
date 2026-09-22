//! Actual FUSE SYMLINK and explicit mounted SDK target/readlink boundaries.
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
                ffi::OsStrExt,
                fs::{FileTypeExt, MetadataExt},
                net::UnixStream,
            },
        },
        path::{Path, PathBuf},
        process::Command,
        time::{Duration, Instant},
    };

    fn check(id: &str) {
        println!("MOUNTED_SYMLINK_CHECK {id} PASS");
    }
    fn kernel(f: &Fixture, script: &str) {
        let out = Command::new("python3")
            .args(["-c", script])
            .arg(f.workspace.mount_path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        print!("{}", String::from_utf8_lossy(&out.stdout));
    }
    fn link(f: &Fixture, name: &[u8], target: &[u8]) -> NodeAttributes {
        let handles = f.workspace.status().unwrap().handles;
        let a = f
            .workspace
            .symlink(f.workspace.root().serial, name, target, deadline())
            .unwrap();
        assert_eq!(
            (a.kind, a.mode, a.size, a.references),
            (NodeKind::Symlink, 0o777, target.len() as u64, 1)
        );
        assert_eq!(f.workspace.status().unwrap().handles, handles);
        a
    }
    fn native_link(f: &Fixture, serial: u64) -> Vec<u8> {
        f.workspace
            .readlink(serial, deadline())
            .unwrap()
            .as_ref()
            .to_vec()
    }
    fn kernel_link(path: &Path) -> Vec<u8> {
        fs::read_link(path).unwrap().as_os_str().as_bytes().to_vec()
    }
    fn kernel_attributes(path: &Path, a: NodeAttributes) {
        let m = fs::symlink_metadata(path).unwrap();
        assert!(m.file_type().is_symlink());
        assert_eq!(
            (
                m.ino(),
                m.mode() & 0o777,
                m.len(),
                m.mtime(),
                m.mtime_nsec()
            ),
            (
                a.serial,
                a.mode,
                a.size,
                a.mtime_seconds,
                i64::from(a.mtime_nanoseconds)
            )
        );
    }
    fn saved(f: &Fixture, root: Root, name: &[u8], target: &[u8], a: NodeAttributes) {
        let Response::Attributes {
            serial,
            kind,
            references,
            size,
            mode,
            mtime,
            nanoseconds,
            ..
        } = f.native.attributes(root, name)
        else {
            panic!("attributes");
        };
        assert_eq!(
            (serial, kind, references, size, mode, mtime, nanoseconds),
            (
                a.serial,
                3,
                1,
                target.len() as u64,
                0o777,
                a.mtime_seconds,
                a.mtime_nanoseconds
            )
        );
        assert_eq!(
            f.native
                .request(
                    Operation::Inspect {
                        root,
                        query: Inspect::Readlink {
                            path: name.to_vec()
                        }
                    },
                    0,
                    &mut std::io::sink()
                )
                .unwrap(),
            Response::Link(target.to_vec())
        );
    }
    fn fixture(gate: Gate) -> Fixture {
        let native = Native::new_fresh(gate);
        let host = WorkspaceHost::new(
            WorkspaceConfig {
                root: PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()),
                max_count: 3,
                memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                disk_budget_bytes: Some(64 * 1024 * 1024),
            },
            native.delivery(),
        )
        .unwrap();
        let workspace = host
            .attach(
                Fixture::options("stage", 31, WorkspaceAccess::LocalEdit),
                deadline(),
            )
            .unwrap();
        assert_eq!(workspace.root().uid, 0);
        Fixture {
            host,
            workspace,
            native,
        }
    }
    #[test]
    #[ignore = "requires actual kernel SYMLINK and explicit 4096-byte READLINK error"]
    fn mounted_symlink_kernel() {
        let f = fixture(Gate::None);
        let before = snapshot(&f);
        let mut readonly = layerfs_fuse::mount(&f.workspace, deadline()).unwrap();
        kernel(
            &f,
            r#"
import errno, os, sys
p=os.fsencode(sys.argv[1])
try: os.symlink(b'target',p+b'/readonly')
except OSError as e: assert e.errno==errno.EROFS
else: raise AssertionError('read-only mount accepted symlink')
print('KERNEL_SYMLINK readonly=EROFS')
"#,
        );
        quiescent(&f);
        readonly.unmount(deadline()).unwrap();
        assert_eq!(count(&f, true), 0);
        let mut mount = writable_mount(&f);
        kernel(
            &f,
            r#"
import errno, os, stat, sys
p=os.fsencode(sys.argv[1]);os.umask(0o077)
for name,target in [(b'relative',b'../missing'),(b'absolute',b'/round48/missing'),(b'opaque',b'\xff/\x80'),(b'maximum',b'x'*4095)]:
    os.symlink(target,p+b'/'+name)
    s=os.lstat(p+b'/'+name)
    assert stat.S_ISLNK(s.st_mode) and s.st_mode & 0o777==0o777 and s.st_size==len(target)
    assert os.readlink(p+b'/'+name)==target
for target,name,expected in [(b'target',b'data.bin',errno.EEXIST),(b'target',b'relative',errno.EEXIST),
    (b'target',b'data.bin/child',errno.ENOTDIR),(b'',b'empty-kernel',errno.ENOENT),
    (b'x'*4096,b'over-kernel',errno.ENAMETOOLONG)]:
    try: os.symlink(target,p+b'/'+name)
    except OSError as e: assert e.errno==expected,(name,e.errno,expected)
    else: raise AssertionError(name+b' accepted')
assert not os.path.lexists(p+b'/empty-kernel') and not os.path.lexists(p+b'/over-kernel')
print('KERNEL_SYMLINK created=4 targets=relative,absolute,opaque,4095 mode=0777 umask=077 uid=0 empty=ENOENT 4096=ENAMETOOLONG Linux-preflight')
"#,
        );
        quiescent(&f);
        let mut all = Vec::new();
        for (name, target) in [
            (b"relative".as_slice(), b"../missing".to_vec()),
            (b"absolute", b"/round48/missing".to_vec()),
            (b"opaque", b"\xff/\x80".to_vec()),
            (b"maximum", vec![b'x'; 4095]),
        ] {
            let a = lookup(&f, f.workspace.root().serial, name);
            kernel_attributes(
                &f.workspace
                    .mount_path()
                    .join(std::str::from_utf8(name).unwrap()),
                a,
            );
            assert_eq!(native_link(&f, a.serial), target);
            all.push((name, target, a));
        }
        assert_eq!(count(&f, true), 4);
        assert_eq!(count(&f, false), 0);
        let target = vec![b'y'; 4096];
        quiescent(&f);
        let native = link(&f, b"sdk-4096", &target);
        assert_eq!(native_link(&f, native.serial), target);
        let path = f.workspace.mount_path().join("sdk-4096");
        kernel_attributes(&path, native);
        assert_eq!(
            fs::read_link(&path).unwrap_err().raw_os_error(),
            Some(nix::libc::ENAMETOOLONG)
        );
        assert_eq!(count(&f, true), 5);
        assert_eq!(count(&f, false), 0);
        assert_eq!(snapshot(&f), before);
        let root = commit(&f);
        for (name, target, a) in &all {
            saved(&f, root, name, target, *a);
            missing(&f, before.effective_root, name);
        }
        saved(&f, root, b"sdk-4096", &target, native);
        missing(&f, before.effective_root, b"sdk-4096");
        assert_eq!(native_link(&f, native.serial), target);
        assert_eq!(
            fs::read_link(&path).unwrap_err().raw_os_error(),
            Some(nix::libc::ENAMETOOLONG),
            "canonical target must not be truncated"
        );
        assert_eq!(count(&f, false), 1);
        quiescent(&f);
        mount.unmount(deadline()).unwrap();
        let mut refs: Vec<_> = all.iter().map(|(_, _, a)| a.serial).collect();
        refs.push(native.serial);
        println!("MKDIR_RESOURCE SDK_target4096_native_exact=true kernel_readlink=ENAMETOOLONG before_and_after_Commit=true no_truncation=true");
        check("actual-kernel-symlinks-and-native4096-explicit-kernel-readlink-boundary");
        close(&f, &refs);
    }

    #[test]
    #[ignore = "requires first kernel empty-target read during captured save and real D1 symlink"]
    fn mounted_symlink_visibility_successor() {
        let f = fixture(Gate::Delivery);
        let before = snapshot(&f);
        let mut mount = writable_mount(&f);
        let path = f.workspace.mount_path();
        assert_eq!(
            fs::symlink_metadata(path.join("sdk-empty"))
                .unwrap_err()
                .kind(),
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
        let sdk = link(&f, b"sdk-empty", b"");
        kernel_attributes(&path.join("sdk-empty"), sdk);
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
        quiescent(&f);
        let workspace = f.workspace.clone();
        let saving = std::thread::spawn(move || workspace.stage(deadline()));
        f.native.wait_entered();
        let calls = f.native.observations.lock().unwrap().operations.len();
        // No earlier READLINK can have populated a kernel target cache.
        assert_eq!(kernel_link(&path.join("sdk-empty")), b"");
        assert_eq!(
            f.native.observations.lock().unwrap().operations.len(),
            calls,
            "captured local target opened another RPC lane"
        );
        f.native.release();
        let stage = saving.join().unwrap().unwrap();
        let frozen = stage.stage().clone();
        saved(&f, frozen.candidate_root, b"sdk-empty", b"", sdk);
        std::os::unix::fs::symlink("born", path.join("born")).unwrap();
        quiescent(&f);
        let born = lookup(&f, f.workspace.root().serial, b"born");
        assert_eq!(kernel_link(&path.join("born")), b"born");
        old.seek(SeekFrom::Start(cookie)).unwrap();
        assert_eq!(directory_tail(&old), tail);
        let one = f.workspace.commit_staged(&stage, deadline()).unwrap();
        let CommitOutcomeWire::Committed(one) = one.outcome else {
            panic!("G Commit");
        };
        assert_eq!(one.root, frozen.candidate_root);
        missing(&f, one.root, b"born");
        assert_eq!(kernel_link(&path.join("sdk-empty")), b"");
        assert_eq!(kernel_link(&path.join("born")), b"born");
        let start = f.native.observations.lock().unwrap().operations.len();
        let root = commit(&f);
        saved(&f, root, b"sdk-empty", b"", sdk);
        saved(&f, root, b"born", b"born", born);
        let observed = f.native.observations.lock().unwrap();
        let targets: Vec<_> = observed.operations[start..]
            .iter()
            .filter_map(|op| match op {
                Operation::ConstructSymlink { target } => Some(target.as_slice()),
                _ => None,
            })
            .collect();
        assert_eq!(targets, vec![b"born".as_slice()]);
        let p = observed.operations[start..]
            .iter()
            .find_map(|op| match op {
                Operation::HistoryCommand(HistoryCommand::Commit(p)) => Some(p),
                _ => None,
            })
            .unwrap();
        assert_eq!(p.new_symlink_serials, vec![born.serial]);
        assert!(p.new_file_serials.is_empty());
        drop(observed);
        old.seek(SeekFrom::Start(cookie)).unwrap();
        assert_eq!(directory_tail(&old), tail);
        assert!(!prefix
            .iter()
            .chain(&tail)
            .any(|row| row.0 == b"sdk-empty" || row.0 == b"born"));
        let names: Vec<_> = fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(names.iter().any(|n| n == "sdk-empty") && names.iter().any(|n| n == "born"));
        assert_eq!(count(&f, true), 2);
        assert_eq!(count(&f, false), 2);
        missing(&f, before.effective_root, b"sdk-empty");
        missing(&f, before.effective_root, b"born");
        drop(stage);
        drop(old);
        quiescent(&f);
        mount.unmount(deadline()).unwrap();
        check("SDK-empty-kernel-readlink-stable-directory-and-G-D1-symlink-Commits");
        close(&f, &[sdk.serial, born.serial]);
    }

    #[test]
    #[ignore = "requires actual mount binding; public projection SYMLINK subset"]
    fn mounted_symlink_permit() {
        let f = fixture(Gate::None);
        let mut mount = writable_mount(&f);
        quiescent(&f);
        let before = f.workspace.status().unwrap();
        let parent = f.workspace.root().serial;
        let old = f.workspace.begin_projection_reply(deadline()).unwrap();
        assert_eq!(
            f.workspace
                .symlink(parent, b"excluded", b"target", deadline()),
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
            permit.symlink(parent, b"bad/name", b"target", deadline()),
            Err(WorkspaceError::InvalidInput)
        );
        assert_eq!(
            permit.symlink(parent, b"used", b"target", deadline()),
            Err(WorkspaceError::InvalidInput)
        );
        assert_eq!(count(&f, true), 0);
        drop(permit);
        let fd = fuse_fd();
        let a = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    deny_notifier_writev(fd, None);
                    let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
                    let a = permit
                        .symlink(parent, b"projected", b"target", deadline())
                        .unwrap();
                    assert_eq!((a.kind, a.mode, a.size), (NodeKind::Symlink, 0o777, 6));
                    let held = f.workspace.status().unwrap();
                    assert_eq!(held.handles, before.handles);
                    assert_eq!(held.projection_replies, before.projection_replies + 1);
                    assert_eq!(
                        f.workspace.symlink(parent, b"held", b"target", deadline()),
                        Err(WorkspaceError::Busy)
                    );
                    assert!(matches!(
                        f.workspace.begin_projection_mutation(deadline()),
                        Err(WorkspaceError::Busy)
                    ));
                    assert_eq!(
                        permit.symlink(parent, b"again", b"target", deadline()),
                        Err(WorkspaceError::InvalidInput)
                    );
                    assert_eq!(count(&f, true), 1);
                    // Public reply-owner release, not an injected kernel reply-send failure.
                    f.workspace.forget(a.serial, 1, ReferenceScope::Projection);
                    assert_eq!(f.workspace.getattr(a.serial), Err(WorkspaceError::NotFound));
                    drop(permit);
                    a
                })
                .join()
                .unwrap()
        });
        let after = f.workspace.status().unwrap();
        assert_eq!(
            (after.handles, after.projection_replies),
            (before.handles, before.projection_replies)
        );
        assert_eq!(after.coherence, Some(CoherenceStatus::Ready));
        assert_eq!(lookup(&f, parent, b"projected"), a);
        assert_eq!(native_link(&f, a.serial), b"target");
        assert_eq!(count(&f, true), 1);
        assert_eq!(count(&f, false), 0);
        let root = commit(&f);
        saved(&f, root, b"projected", b"target", a);
        mount.unmount(deadline()).unwrap();
        println!("PROJECTED_SYMLINK no_notifier=true reply_owner_cleanup=public-API kernel_reply_send_fault=NOT_RUN");
        check("single-use-projected-symlink-held-reply-exclusion-no-notifier-and-ref-cleanup");
        close(&f, &[a.serial]);
    }

    #[test]
    #[ignore = "requires actual entry-notification EIO on one dedicated thread"]
    fn mounted_symlink_notification_failure() {
        let f = fixture(Gate::None);
        let before_branch = snapshot(&f);
        let mut mount = writable_mount(&f);
        assert_eq!(
            fs::symlink_metadata(f.workspace.mount_path().join("retained"))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
        quiescent(&f);
        let before = f.workspace.status().unwrap();
        let fd = fuse_fd();
        let workers: Vec<_> = fs::read_dir("/proc/self/task")
            .unwrap()
            .filter_map(|e| {
                let tid: i32 = e.unwrap().file_name().to_str().unwrap().parse().unwrap();
                seccomp_filter_count(tid).map(|count| (tid, count))
            })
            .collect();
        let (mut ordinary, mut reader) = UnixStream::pair().unwrap();
        let (error, tid, old_filters, new_filters) = std::thread::scope(|scope| {
            let child = scope.spawn(|| {
                let tid = unsafe { nix::libc::syscall(nix::libc::SYS_gettid) as i32 };
                let old = seccomp_filter_count(tid).expect("faulting caller is live");
                deny_notifier_writev(fd, Some(4));
                let new = seccomp_filter_count(tid).expect("faulting caller is live");
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
                    .symlink(
                        f.workspace.root().serial,
                        b"retained",
                        b"\xff/kept",
                        deadline(),
                    )
                    .unwrap_err();
                (error, tid, old, new)
            });
            let mut bytes = [0; 4];
            reader.read_exact(&mut bytes).unwrap();
            assert_eq!(&bytes, b"abcd");
            child.join().unwrap()
        });
        assert_eq!(new_filters, old_filters + 1);
        for (tid, filters) in workers {
            if let Some(actual) = seccomp_filter_count(tid) {
                assert_eq!(actual, filters, "filter spread to surviving worker");
            }
        }
        let WorkspaceError::Coherence(failure) = error else {
            panic!("{error:?}");
        };
        assert_eq!(failure.raw_os_error, Some(nix::libc::EIO));
        assert!(!failure.notifier_returned_ok);
        assert_eq!(failure.published_handle, None);
        assert_eq!(failure.receipt.accepted_bytes, 0);
        assert_eq!(failure.receipt.revision, before.revision + 1);
        let failed = f.workspace.status().unwrap();
        assert_eq!(failed.coherence, Some(CoherenceStatus::Failed(failure)));
        assert_eq!(failed.handles, before.handles);
        assert_eq!(
            f.workspace.getattr(failure.receipt.inode),
            Err(WorkspaceError::NotFound),
            "unreturned Local reference must be released"
        );
        assert_eq!(
            f.workspace.symlink(
                f.workspace.root().serial,
                b"retained",
                b"different",
                deadline()
            ),
            Err(WorkspaceError::Busy)
        );
        assert!(matches!(
            f.workspace.begin_projection_mutation(deadline()),
            Err(WorkspaceError::Busy)
        ));
        let a = lookup(&f, f.workspace.root().serial, b"retained");
        assert_eq!(a.serial, failure.receipt.inode);
        assert_eq!(native_link(&f, a.serial), b"\xff/kept");
        assert_eq!(count(&f, true), 1);
        assert_eq!(count(&f, false), 0);
        assert_eq!(snapshot(&f), before_branch);
        println!("MKDIR_RESOURCE symlink_notification fd={fd} iovcnt=4 tid={tid} filters={old_filters}->{new_filters} parent_notification_order=source-reviewed-not-independently-counted {failure:?}");
        drop(ordinary);
        drop(reader);
        mount.unmount(deadline()).unwrap();
        assert!(!mounted(&f));
        assert_eq!(f.workspace.status().unwrap().coherence, None);
        let mut repaired = writable_mount(&f);
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Ready)
        );
        kernel_attributes(&f.workspace.mount_path().join("retained"), a);
        assert_eq!(
            kernel_link(&f.workspace.mount_path().join("retained")),
            b"\xff/kept"
        );
        let root = commit(&f);
        saved(&f, root, b"retained", b"\xff/kept", a);
        assert_eq!(
            kernel_link(&f.workspace.mount_path().join("retained")),
            b"\xff/kept"
        );
        assert_eq!(
            count(&f, true),
            1,
            "remount must not replay symlink/Reserve"
        );
        assert_eq!(count(&f, false), 1);
        missing(&f, before_branch.effective_root, b"retained");
        quiescent(&f);
        repaired.unmount(deadline()).unwrap();
        check("entry-notifier-failure-retains-symlink-target-drops-ref-and-remount-repairs");
        close(&f, &[a.serial]);
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
}
