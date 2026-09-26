//! Actual mounted CREATE plus separately labeled public projection-origin proofs.
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
        io::{IoSlice, Read, Seek, SeekFrom, Write},
        os::{
            fd::AsRawFd,
            unix::{
                fs::{FileExt, FileTypeExt, MetadataExt, OpenOptionsExt},
                net::UnixStream,
            },
        },
        path::{Path, PathBuf},
        process::Command,
        time::{Duration, Instant},
    };
    fn check(id: &str) {
        println!("MOUNTED_CREATE_CHECK {id} PASS");
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
    fn options(exclusive: bool, truncate: bool) -> FileCreateOptions {
        FileCreateOptions {
            mode: 0o640,
            umask: 0,
            exclusive,
            open: FileOpenOptions {
                access: FileAccess::ReadWrite,
                append: false,
                truncate,
            },
        }
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
    #[test]
    #[ignore = "requires actual kernel CREATE and initial file-descriptor rights"]
    fn mounted_create_kernel() {
        let f = fixture(Gate::None);
        let before = snapshot(&f);
        let mut mount = writable_mount(&f);
        kernel(
            &f,
            r#"
import errno, os, sys
p=sys.argv[1]
os.umask(0o027)
fd=os.open(p+'/empty',os.O_CREAT|os.O_EXCL|os.O_RDONLY,0o666)
assert os.fstat(fd).st_mode & 0o777 == 0o640
assert os.read(fd,1)==b''
try: os.write(fd,b'x')
except OSError as e: assert e.errno==errno.EBADF
else: raise AssertionError('read-only created fd wrote')
os.close(fd)
fd=os.open(p+'/restricted',os.O_CREAT|os.O_EXCL|os.O_RDWR,0o400)
assert os.fstat(fd).st_mode & 0o777 == 0o400
assert os.write(fd,b'abcdef')==6
os.ftruncate(fd,3)
os.ftruncate(fd,6)
assert os.pread(fd,6,0)==b'abc\0\0\0'
os.close(fd)
fd=os.open(p+'/zero',os.O_CREAT|os.O_EXCL|os.O_WRONLY|os.O_APPEND,0)
assert os.fstat(fd).st_mode & 0o777 == 0
assert os.write(fd,b'A')==1
os.lseek(fd,0,os.SEEK_SET)
assert os.write(fd,b'B')==1
os.ftruncate(fd,4)
assert os.write(fd,b'C')==1
try: os.pread(fd,1,0)
except OSError as e: assert e.errno==errno.EBADF
else: raise AssertionError('write-only created fd read')
assert os.fstat(fd).st_size==5
os.close(fd)
print('KERNEL_CREATE files=3 umask=027 initial_fd_modes=0400,000 uid=0 later_unprivileged_open=NOT_RUN')
"#,
        );
        drained(&f);
        let mut refs = Vec::new();
        let mut expected = Vec::new();
        for (name, mode, bytes) in [
            (b"empty".as_slice(), 0o640, b"".as_slice()),
            (b"restricted".as_slice(), 0o400, b"abc\0\0\0".as_slice()),
            (b"zero".as_slice(), 0, b"AB\0\0C".as_slice()),
        ] {
            let a = lookup(&f, f.workspace.root().serial, name);
            assert_eq!((a.mode, a.size), (mode, bytes.len() as u64));
            kernel_metadata(
                &f.workspace
                    .mount_path()
                    .join(std::str::from_utf8(name).unwrap()),
                a,
            );
            refs.push(a.serial);
            expected.push((name, bytes, a));
            missing(&f, before.effective_root, name);
        }
        assert_eq!(count(&f, true), 3);
        assert_eq!(count(&f, false), 0);
        assert_eq!(snapshot(&f), before);
        let root = commit(&f);
        for (name, bytes, a) in expected {
            saved(&f, root, name, bytes, a);
        }
        assert_eq!(count(&f, false), 1);
        mount.unmount(deadline()).unwrap();
        check("actual-created-fd-empty-restrictive-mode-umask-access-append-resize-and-Commit");
        close(&f, &refs);
    }

    #[test]
    #[ignore = "requires real kernel open refusals and explicitly labeled projected existing CREATE"]
    fn mounted_create_existing() {
        let f = fixture(Gate::None);
        let before = snapshot(&f);
        let baseline = f.workspace.status().unwrap();
        let mut ro = layerfs_fuse::mount(&f.workspace, deadline()).unwrap();
        kernel(
            &f,
            r#"
import errno, os, sys
try: os.open(sys.argv[1]+'/readonly-new',os.O_CREAT|os.O_RDWR,0o600)
except OSError as e: assert e.errno==errno.EROFS
else: raise AssertionError('read-only mount created')
print('KERNEL_CREATE_REFUSAL readonly=EROFS')
"#,
        );
        drained(&f);
        ro.unmount(deadline()).unwrap();
        assert_eq!(f.workspace.status().unwrap().revision, baseline.revision);
        assert_eq!(count(&f, true), 0);
        let mut mount = writable_mount(&f);
        let a = lookup(&f, f.workspace.root().serial, b"data.bin");
        kernel(
            &f,
            r#"
import errno, os, sys
p=sys.argv[1]
def refused(name, flags, error):
    try: fd=os.open(p+'/'+name,flags,0o600)
    except OSError as e: assert e.errno==error,(name,e.errno,error)
    else: os.close(fd); raise AssertionError(name+' accepted')
refused('data.bin',os.O_CREAT|os.O_EXCL|os.O_RDWR,errno.EEXIST)
refused('directory',os.O_CREAT|os.O_RDWR,errno.EISDIR)
refused('link',os.O_CREAT|os.O_EXCL|os.O_RDWR,errno.EEXIST)
refused('link',os.O_CREAT|os.O_NOFOLLOW|os.O_RDWR,errno.ELOOP)
refused('sync-new',os.O_CREAT|os.O_EXCL|os.O_RDWR|os.O_SYNC,errno.EOPNOTSUPP)
assert not os.path.exists(p+'/sync-new')
fd=os.open(p+'/data.bin',os.O_CREAT|os.O_TRUNC|os.O_RDWR,0)
assert os.fstat(fd).st_size==0
assert os.write(fd,b'existing')==8
assert os.fstat(fd).st_mode & 0o777==0o644
assert os.fstat(fd).st_ino==os.stat(p+'/alias').st_ino
os.close(fd)
print('KERNEL_OPEN_EXISTING O_CREAT_path=kernel-selected-OPEN-or-CREATE explicit_CREATE_claim=false')
"#,
        );
        drained(&f);
        assert_eq!(f.workspace.getattr(a.serial).unwrap().size, 8);
        assert_eq!(
            fs::read(f.workspace.mount_path().join("alias")).unwrap(),
            b"existing"
        );
        drained(&f);
        assert_eq!(count(&f, true), 0);
        let parent = f.workspace.getattr(f.workspace.root().serial).unwrap();
        let fd = fuse_fd();
        let (exact, handle) = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    deny_notifier_writev(fd, None);
                    let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
                    let mut opt = options(false, true);
                    opt.mode = 0; // Valid creation fields cannot replace an existing inode's mode.
                    let result = permit
                        .create_file(f.workspace.root().serial, b"data.bin", opt, deadline())
                        .unwrap();
                    assert_eq!(
                        permit.create_file(
                            f.workspace.root().serial,
                            b"again",
                            options(true, false),
                            deadline()
                        ),
                        Err(WorkspaceError::InvalidInput)
                    );
                    result
                })
                .join()
                .unwrap()
        });
        assert_eq!(
            (exact.serial, exact.mode, exact.size),
            (a.serial, a.mode, 0)
        );
        assert_eq!(f.workspace.handle_attributes(handle).unwrap(), exact);
        assert_eq!(
            f.workspace.getattr(f.workspace.root().serial).unwrap(),
            parent
        );
        f.workspace.release(handle).unwrap();
        f.workspace.forget(a.serial, 1, ReferenceScope::Projection);
        assert_eq!(count(&f, true), 0);
        assert_eq!(count(&f, false), 0);
        assert_eq!(snapshot(&f), before);
        // This is an explicit public projection API call, not an observed kernel CREATE opcode.
        println!("PROJECTED_CREATE existing_truncate=true same_inode=true no_notifier=true");
        let root = commit(&f);
        let Response::Attributes {
            content,
            size,
            mode,
            serial,
            references,
            ..
        } = f.native.attributes(root, b"data.bin")
        else {
            panic!("attributes");
        };
        assert_eq!((serial, mode, size, references), (a.serial, a.mode, 0, 2));
        assert_eq!(f.native.bytes(content, 0, 0), b"");
        assert_eq!(attr(f.native.attributes(root, b"alias")).1, content);
        mount.unmount(deadline()).unwrap();
        check("kernel-existing-excl-wrongkind-readonly-flags-and-projected-existing-create");
        close(&f, &[a.serial]);
    }

    #[test]
    #[ignore = "requires SDK entry notification and stable actual directory cookies"]
    fn mounted_create_visibility() {
        let f = fixture(Gate::None);
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
        let (sdk, handle) = f
            .workspace
            .create_file(
                f.workspace.root().serial,
                b"sdk",
                options(true, false),
                deadline(),
            )
            .unwrap();
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Ready)
        );
        kernel_metadata(&path.join("sdk"), sdk);
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
        f.workspace
            .write_file(handle, 0, &f.own(b"sdk"), deadline())
            .unwrap();
        assert_eq!(fs::read(path.join("sdk")).unwrap(), b"sdk");
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(path.join("kernel"))
            .unwrap();
        file.write_all(b"kernel").unwrap();
        drop(file);
        quiescent(&f);
        let kernel = lookup(&f, f.workspace.root().serial, b"kernel");
        kernel_metadata(&path.join("kernel"), kernel);
        old.seek(SeekFrom::Start(cookie)).unwrap();
        assert_eq!(directory_tail(&old), tail);
        let names: Vec<_> = fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        for name in ["sdk", "kernel"] {
            assert!(names.iter().any(|n| n == name));
        }
        assert!(!prefix
            .iter()
            .chain(&tail)
            .any(|r| r.0 == b"sdk" || r.0 == b"kernel"));
        assert_eq!(count(&f, true), 2);
        assert_eq!(count(&f, false), 0);
        let sdk = f.workspace.handle_attributes(handle).unwrap();
        let root = commit(&f);
        saved(&f, root, b"sdk", b"sdk", sdk);
        saved(&f, root, b"kernel", b"kernel", kernel);
        old.seek(SeekFrom::Start(cookie)).unwrap();
        assert_eq!(directory_tail(&old), tail);
        missing(&f, before.effective_root, b"sdk");
        missing(&f, before.effective_root, b"kernel");
        drop(old);
        f.workspace.release(handle).unwrap();
        drained(&f);
        mount.unmount(deadline()).unwrap();
        check("SDK-create-negative-lookup-parent-attrs-and-stable-kernel-directory-handle");
        close(&f, &[sdk.serial, kernel.serial]);
    }

    #[test]
    #[ignore = "requires actual projection binding and external no-notifier fault"]
    fn mounted_create_permit() {
        let f = fixture(Gate::None);
        let mut mount = writable_mount(&f);
        quiescent(&f);
        let before = f.workspace.status().unwrap();
        let old = f.workspace.begin_projection_reply(deadline()).unwrap();
        assert_eq!(
            f.workspace.create_file(
                f.workspace.root().serial,
                b"excluded",
                options(true, false),
                deadline()
            ),
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
            permit.create_file(
                f.workspace.root().serial,
                b"bad/name",
                options(true, false),
                deadline()
            ),
            Err(WorkspaceError::InvalidInput)
        );
        assert_eq!(
            permit.create_file(
                f.workspace.root().serial,
                b"used",
                options(true, false),
                deadline()
            ),
            Err(WorkspaceError::InvalidInput)
        );
        assert_eq!(count(&f, true), 0);
        drop(permit);
        let fd = fuse_fd();
        let a = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    // All notifier writev calls on this owned FUSE fd must be absent.
                    deny_notifier_writev(fd, None);
                    let mut permit = f.workspace.begin_projection_mutation(deadline()).unwrap();
                    let (a, handle) = permit
                        .create_file(
                            f.workspace.root().serial,
                            b"projected",
                            options(true, false),
                            deadline(),
                        )
                        .unwrap();
                    assert_eq!(f.workspace.handle_attributes(handle).unwrap(), a);
                    let held = f.workspace.status().unwrap();
                    assert_eq!(held.projection_handles, before.projection_handles + 1);
                    assert_eq!(held.projection_replies, before.projection_replies + 1);
                    assert_eq!(
                        f.workspace.create_file(
                            f.workspace.root().serial,
                            b"held",
                            options(true, false),
                            deadline()
                        ),
                        Err(WorkspaceError::Busy)
                    );
                    assert!(matches!(
                        f.workspace.begin_projection_mutation(deadline()),
                        Err(WorkspaceError::Busy)
                    ));
                    assert_eq!(
                        permit.create_file(
                            f.workspace.root().serial,
                            b"again",
                            options(true, false),
                            deadline()
                        ),
                        Err(WorkspaceError::InvalidInput)
                    );
                    assert_eq!(count(&f, true), 1);
                    // Public reply-owner cleanup subset, not an injected kernel reply-send failure.
                    f.workspace.release(handle).unwrap();
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
            (
                after.handles,
                after.projection_handles,
                after.projection_replies
            ),
            (
                before.handles,
                before.projection_handles,
                before.projection_replies
            )
        );
        assert_eq!(after.coherence, Some(CoherenceStatus::Ready));
        assert_eq!(lookup(&f, f.workspace.root().serial, b"projected"), a);
        assert_eq!(count(&f, true), 1);
        assert_eq!(count(&f, false), 0);
        let root = commit(&f);
        saved(&f, root, b"projected", b"", a);
        mount.unmount(deadline()).unwrap();
        println!("PROJECTED_CREATE reply_owner_cleanup=public-API kernel_reply_send_fault=NOT_RUN");
        check("single-use-create-permit-held-reply-exclusion-no-notifier-and-exact-custody");
        close(&f, &[a.serial]);
    }

    #[test]
    #[ignore = "requires actual entry-notifier EIO on a dedicated caller thread"]
    fn mounted_create_notification_failure() {
        let f = fixture(Gate::None);
        let before_branch = snapshot(&f);
        let mut mount = writable_mount(&f);
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
            .filter_map(|e| {
                let tid: i32 = e.unwrap().file_name().to_str().unwrap().parse().unwrap();
                seccomp_filter_count(tid).map(|filters| (tid, filters))
            })
            .collect();
        let (mut ordinary, mut reader) = UnixStream::pair().unwrap();
        let (error, tid, old_filters, new_filters) = std::thread::scope(|scope| {
            let child = scope.spawn(|| {
                let tid = unsafe { nix::libc::syscall(nix::libc::SYS_gettid) as i32 };
                let old = seccomp_filter_count(tid).expect("faulting caller is still live");
                deny_notifier_writev(fd, Some(4));
                let new = seccomp_filter_count(tid).expect("faulting caller is still live");
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
                    .create_file(
                        f.workspace.root().serial,
                        b"retained",
                        options(true, false),
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
        // join completes the faulting user code; procfs removal can finish later.
        for (worker, filters) in workers {
            if let Some(actual) = seccomp_filter_count(worker) {
                assert_eq!(actual, filters, "filter spread to a surviving thread");
            }
        }
        let WorkspaceError::Coherence(failure) = error else {
            panic!("{error:?}");
        };
        assert_eq!(failure.raw_os_error, Some(nix::libc::EIO));
        assert!(!failure.notifier_returned_ok);
        assert_eq!(failure.receipt.accepted_bytes, 0);
        assert_eq!(failure.receipt.revision, before.revision + 1);
        let handle = failure
            .published_handle
            .expect("published READY Local handle must remain owned");
        let a = f.workspace.handle_attributes(handle).unwrap();
        assert_eq!((a.serial, a.size), (failure.receipt.inode, 0));
        assert!(f.read(handle, 0, 1).is_empty());
        let failed = f.workspace.status().unwrap();
        assert_eq!(failed.coherence, Some(CoherenceStatus::Failed(failure)));
        assert_eq!(failed.handles, before.handles + 1);
        assert_eq!(failed.projection_handles, before.projection_handles);
        assert_eq!(
            f.workspace.create_file(
                f.workspace.root().serial,
                b"retained",
                options(true, false),
                deadline()
            ),
            Err(WorkspaceError::Busy)
        );
        assert!(matches!(
            f.workspace.begin_projection_mutation(deadline()),
            Err(WorkspaceError::Busy)
        ));
        let payload = f.own(b"blocked");
        assert_eq!(
            f.workspace.write_file(handle, 0, &payload, deadline()),
            Err(WorkspaceError::Busy)
        );
        drop(payload);
        assert_eq!(
            f.workspace.status().unwrap().revision,
            failure.receipt.revision
        );
        assert_eq!(count(&f, true), 1);
        assert_eq!(count(&f, false), 0);
        assert_eq!(snapshot(&f), before_branch);
        println!("MKDIR_RESOURCE create_notification fd={fd} iovcnt=4 tid={tid} filters={old_filters}->{new_filters} {failure:?}");
        drop(ordinary);
        drop(reader);
        mount.unmount(deadline()).unwrap();
        assert!(!mounted(&f));
        assert_eq!(f.workspace.status().unwrap().coherence, None);
        assert_eq!(f.workspace.handle_attributes(handle).unwrap(), a);
        let mut repaired = writable_mount(&f);
        assert_eq!(
            f.workspace.status().unwrap().coherence,
            Some(CoherenceStatus::Ready)
        );
        let file = File::open(f.workspace.mount_path().join("retained")).unwrap();
        assert_eq!(file.metadata().unwrap().ino(), a.serial);
        assert!(bytes(&file, 0).is_empty());
        quiescent(&f);
        f.workspace
            .write_file(handle, 0, &f.own(b"repaired"), deadline())
            .unwrap();
        assert_eq!(bytes(&file, 8), b"repaired");
        let a = f.workspace.handle_attributes(handle).unwrap();
        let root = commit(&f);
        saved(&f, root, b"retained", b"repaired", a);
        missing(&f, before_branch.effective_root, b"retained");
        assert_eq!(count(&f, true), 1, "repair must not replay Reserve/create");
        assert_eq!(count(&f, false), 1);
        drop(file);
        drained(&f);
        repaired.unmount(deadline()).unwrap();
        f.workspace.release(handle).unwrap();
        assert_eq!(
            f.workspace.getattr(a.serial),
            Err(WorkspaceError::NotFound),
            "no unreturned Local lookup reference survived the failure"
        );
        check("entry-notifier-failure-retains-name-handle-drops-lookup-and-remount-repairs");
        close(&f, &[]);
    }

    #[test]
    #[ignore = "requires created kernel FDs and actual Stage/Commit captured reconciliation"]
    fn mounted_create_successor() {
        let f = fixture(Gate::Delivery);
        let before = snapshot(&f);
        let mut mount = writable_mount(&f);
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .mode(0o640)
            .open(f.workspace.mount_path().join("captured"))
            .unwrap();
        file.write_all_at(b"G0tail", 0).unwrap();
        quiescent(&f);
        let a = lookup(&f, f.workspace.root().serial, b"captured");
        let workspace = f.workspace.clone();
        let saving = std::thread::spawn(move || workspace.stage(deadline()));
        f.native.wait_entered();
        // The FD is already open: no new path lookup while remote admission is occupied.
        file.write_all_at(b"D1", 0).unwrap();
        assert_eq!(bytes(&file, 6), b"D1tail");
        f.native.release();
        let stage = saving.join().unwrap().unwrap();
        let frozen = stage.stage().clone();
        let Response::Attributes {
            content: g_content,
            metadata: g_metadata,
            ..
        } = f.native.attributes(frozen.candidate_root, b"captured")
        else {
            panic!("G attributes");
        };
        assert_eq!(f.native.bytes(g_content, 0, 6), b"G0tail");
        // A fresh D1-born file reserves only after the captured Stage is ready.
        let born_file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(f.workspace.mount_path().join("born"))
            .unwrap();
        born_file.write_all_at(b"born", 0).unwrap();
        quiescent(&f);
        let born = lookup(&f, f.workspace.root().serial, b"born");
        let live = f.workspace.getattr(a.serial).unwrap();
        let one = f.workspace.commit_staged(&stage, deadline()).unwrap();
        let CommitOutcomeWire::Committed(one_commit) = &one.outcome else {
            panic!("first Commit");
        };
        assert_eq!(one_commit.root, frozen.candidate_root);
        assert_eq!(one.stage_token, Some(frozen.token));
        assert_eq!(bytes(&file, 6), b"D1tail");
        assert_eq!(bytes(&born_file, 4), b"born");
        missing(&f, one_commit.root, b"born");
        let start = f.native.observations.lock().unwrap().operations.len();
        let root = commit(&f);
        saved(&f, root, b"captured", b"D1tail", live);
        saved(&f, root, b"born", b"born", born);
        assert_eq!(bytes(&file, 6), b"D1tail");
        assert_eq!(bytes(&born_file, 4), b"born");
        let observed = f.native.observations.lock().unwrap();
        assert!(observed.operations[start..].iter().any(
            |op| matches!(op, Operation::SaveFile { base: Some(root), .. } if *root == g_content)
        ));
        assert!(observed.operations[start..].iter().any(
            |op| matches!(op, Operation::UpdatePortableMetadata { base, .. } if *base == g_metadata)
        ));
        let constructed: Vec<_> = observed.operations[start..]
            .iter()
            .filter_map(|op| match op {
                Operation::SaveFile {
                    base: None, length, ..
                } => Some(*length),
                _ => None,
            })
            .collect();
        assert_eq!(constructed, vec![4]);
        let p = observed.operations[start..]
            .iter()
            .find_map(|op| match op {
                Operation::HistoryCommand(HistoryCommand::Commit(p)) => Some(p),
                _ => None,
            })
            .unwrap();
        assert_eq!(p.new_file_serials, vec![born.serial]);
        assert_eq!(p.base, frozen.candidate_root);
        drop(observed);
        assert_eq!(count(&f, true), 2);
        assert_eq!(count(&f, false), 2);
        missing(&f, before.effective_root, b"captured");
        missing(&f, before.effective_root, b"born");
        drop(stage);
        drop(file);
        drop(born_file);
        drained(&f);
        mount.unmount(deadline()).unwrap();
        check("created-kernel-fd-survives-captured-G-D1-and-two-explicit-Commits");
        close(&f, &[a.serial, born.serial]);
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
    fn drained(f: &Fixture) {
        let end = deadline();
        loop {
            let s = f.workspace.status().unwrap();
            if s.projection_handles == 0 && s.projection_replies == 0 {
                break;
            }
            assert!(Instant::now() < end, "{s:?}");
            std::thread::yield_now();
        }
    }
    fn bytes(file: &File, count: usize) -> Vec<u8> {
        let mut out = vec![0; count];
        file.read_exact_at(&mut out, 0).unwrap();
        out
    }
    fn saved(f: &Fixture, root: Root, path: &[u8], expected: &[u8], a: NodeAttributes) {
        let Response::Attributes {
            serial,
            kind,
            references,
            mode,
            mtime,
            nanoseconds,
            size,
            content,
            ..
        } = f.native.attributes(root, path)
        else {
            panic!("attributes");
        };
        assert_eq!(
            (serial, kind, references, mode, mtime, nanoseconds, size),
            (
                a.serial,
                1,
                1,
                a.mode,
                a.mtime_seconds,
                a.mtime_nanoseconds,
                expected.len() as u64
            )
        );
        assert_eq!(f.native.bytes(content, 0, expected.len()), expected);
    }
    fn kernel_metadata(path: &Path, a: NodeAttributes) {
        let m = fs::metadata(path).unwrap();
        assert!(m.is_file());
        assert_eq!(
            (
                m.ino(),
                m.len(),
                m.mode() & 0o777,
                m.mtime(),
                m.mtime_nsec()
            ),
            (
                a.serial,
                a.size,
                a.mode,
                a.mtime_seconds,
                i64::from(a.mtime_nanoseconds)
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
