//! Attachment custody through public APIs, with native faults outside the product.
#![cfg(unix)]
use layerfs_bridge::contract::*;
use layerfs_workspace::*;
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(1);
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn root() -> Response {
    Response::Attributes {
        serial: 7,
        kind: 2,
        references: 0,
        content: [1; 32],
        metadata: [2; 32],
        mode: 0o755,
        mtime: 0,
        nanoseconds: 0,
        size: 0,
    }
}
struct Fixture {
    path: PathBuf,
    host: WorkspaceHost,
}
impl Fixture {
    fn new(delivery: OperationDelivery) -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "layerfs-attachment-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let host = WorkspaceHost::new(
            WorkspaceConfig {
                root: path.clone(),
                max_count: 4,
                memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                disk_budget_bytes: None,
            },
            delivery,
        )
        .unwrap();
        Self { path, host }
    }
    fn options(&self, id: &str, incarnation: u8) -> AttachOptions {
        use std::os::unix::fs::MetadataExt;
        let owner = fs::metadata(&self.path).unwrap();
        AttachOptions {
            access: WorkspaceAccess::ReadOnly,
            id: id.into(),
            incarnation: [incarnation; 32],
            store: 1,
            base: Base::Root([1; 32]),
            owner_uid: owner.uid(),
            owner_gid: owner.gid(),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn exact_attachment_observation_preserves_the_existing_owner() {
    let f = Fixture::new(Arc::new(|_, _, _, _| Ok(root())));
    for (id, incarnation, expected) in [
        ("missing", [1; 32], WorkspaceError::NotFound),
        ("../invalid", [1; 32], WorkspaceError::InvalidInput),
        ("valid", [0; 32], WorkspaceError::InvalidInput),
    ] {
        assert!(matches!(f.host.attachment(id, incarnation), Err(error) if error == expected));
        assert_eq!(
            f.host.cleanup_failed_attach(id, incarnation, deadline()),
            Err(expected)
        );
    }
    let workspace = f.host.attach(f.options("exact", 1), deadline()).unwrap();
    assert!(matches!(
        f.host.attachment("exact", [2; 32]),
        Err(WorkspaceError::NotFound)
    ));
    assert_eq!(
        f.host.cleanup_failed_attach("exact", [2; 32], deadline()),
        Err(WorkspaceError::NotFound)
    );
    let Attachment::Attached(observed) = f.host.attachment("exact", [1; 32]).unwrap() else {
        panic!("healthy owner absent")
    };
    assert_eq!(observed.root(), workspace.root());
    assert_eq!(observed.mount_path(), workspace.mount_path());
    let mut lease = observed.reserve_mount().unwrap();
    assert!(workspace.status().unwrap().mounted);
    assert_eq!(
        f.host.cleanup_failed_attach("exact", [1; 32], deadline()),
        Err(WorkspaceError::Busy)
    );
    lease.stop_admission();
    lease.finish().unwrap();
    workspace.close_clean().unwrap();
    assert!(observed.status().unwrap().closed);
    assert!(matches!(
        f.host.attachment("exact", [1; 32]),
        Err(WorkspaceError::NotFound)
    ));
}

#[test]
fn active_attachment_is_observable_and_refuses_cleanup_without_holding_registry() {
    let (entered, arrival) = mpsc::sync_channel(1);
    let (release, wait) = mpsc::sync_channel(1);
    let wait = Mutex::new(wait);
    let f = Fixture::new(Arc::new(move |_, _, _, _| {
        entered.send(()).unwrap();
        wait.lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        Ok(root())
    }));
    let options = f.options("active", 1);
    let workspace = std::thread::scope(|scope| {
        let caller = scope.spawn(|| f.host.attach(options, deadline()));
        arrival.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(
            f.host.attachment("active", [1; 32]),
            Ok(Attachment::Attaching)
        ));
        assert_eq!(
            f.host.cleanup_failed_attach("active", [1; 32], deadline()),
            Err(WorkspaceError::Busy)
        );
        assert!(matches!(
            f.host.attach(f.options("active", 2), deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert!(matches!(
            f.host.attach(f.options("other", 1), deadline()),
            Err(WorkspaceError::Busy)
        ));
        release.send(()).unwrap();
        caller.join().unwrap().unwrap()
    });
    workspace.close_clean().unwrap();
}

#[test]
fn unacquired_failure_and_expired_admission_leave_no_attachment() {
    let calls = Arc::new(AtomicU64::new(0));
    let count = calls.clone();
    let f = Fixture::new(Arc::new(move |_, _, _, _| {
        count.fetch_add(1, Ordering::Relaxed);
        Err(Code::Denied.into())
    }));
    assert!(matches!(
        f.host.attach(f.options("failed", 1), Instant::now()),
        Err(WorkspaceError::Deadline)
    ));
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert!(matches!(
        f.host.attach(f.options("failed", 1), deadline()),
        Err(WorkspaceError::Service(Failure {
            code: Code::Denied,
            ..
        }))
    ));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(matches!(
        f.host.attachment("failed", [1; 32]),
        Err(WorkspaceError::NotFound)
    ));
    assert!(!f.path.join("workspace/failed").exists());
}

#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use nix::libc;
    use std::os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::fs::MetadataExt,
    };

    fn check(id: &str) {
        println!("ATTACHMENT_CHECK {id} PASS");
    }
    fn tid() -> u32 {
        unsafe { libc::syscall(libc::SYS_gettid) as u32 }
    }
    fn filters(task: u32) -> u32 {
        fs::read_to_string(format!("/proc/self/task/{task}/status"))
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("Seccomp_filters:"))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }
    fn removal_calls() -> &'static [i64] {
        // aarch64 implements rmdir through unlinkat; no rmdir syscall exists.
        &[
            libc::SYS_unlinkat,
            #[cfg(target_arch = "x86_64")]
            libc::SYS_rmdir,
        ]
    }
    fn arch() -> u32 {
        assert!(cfg!(target_endian = "little"));
        match std::env::consts::ARCH {
            "aarch64" => 0xc00000b7,
            "x86_64" => 0xc000003e,
            other => panic!("unsupported native fault architecture {other}"),
        }
    }
    fn syscall_filter(calls: &[i64], notify: bool) -> i32 {
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
        let mut code = vec![
            stmt(0x20, 4),
            jump(arch(), (2 * calls.len() + 1).try_into().unwrap()),
            stmt(0x20, 0),
        ];
        for &call in calls {
            code.push(jump(call.try_into().unwrap(), 1));
            code.push(stmt(
                0x06,
                if notify {
                    libc::SECCOMP_RET_USER_NOTIF
                } else {
                    libc::SECCOMP_RET_ERRNO | libc::EPERM as u32
                },
            ));
        }
        code.push(stmt(0x06, libc::SECCOMP_RET_ALLOW));
        let program = libc::sock_fprog {
            len: code.len().try_into().unwrap(),
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
            // Filter only this dedicated caller thread. No TSYNC or product hook.
            let flags = if notify {
                libc::SECCOMP_FILTER_FLAG_NEW_LISTENER
            } else {
                0
            };
            let result = libc::syscall(
                libc::SYS_seccomp,
                libc::SECCOMP_SET_MODE_FILTER,
                flags,
                &program,
            );
            assert!(result >= 0, "{}", std::io::Error::last_os_error());
            result.try_into().unwrap()
        }
    }
    fn notification(listener: &OwnedFd, end: Instant) -> libc::seccomp_notif {
        let mut sizes: libc::seccomp_notif_sizes = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe {
                libc::syscall(
                    libc::SYS_seccomp,
                    libc::SECCOMP_GET_NOTIF_SIZES,
                    0,
                    &mut sizes,
                )
            },
            0
        );
        assert_eq!(
            usize::from(sizes.seccomp_notif),
            std::mem::size_of::<libc::seccomp_notif>()
        );
        assert_eq!(
            usize::from(sizes.seccomp_notif_resp),
            std::mem::size_of::<libc::seccomp_notif_resp>()
        );
        let mut poll = libc::pollfd {
            fd: listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let remaining = end
            .checked_duration_since(Instant::now())
            .expect("notification arrived after deadline");
        assert_eq!(
            unsafe { libc::poll(&mut poll, 1, remaining.as_millis().try_into().unwrap()) },
            1
        );
        assert_ne!(poll.revents & libc::POLLIN, 0);
        let mut event: libc::seccomp_notif = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe {
                libc::ioctl(
                    listener.as_raw_fd(),
                    libc::SECCOMP_IOCTL_NOTIF_RECV,
                    &mut event,
                )
            },
            0
        );
        event
    }
    fn continue_call(listener: &OwnedFd, id: u64) {
        let mut response = libc::seccomp_notif_resp {
            id,
            val: 0,
            error: 0,
            flags: libc::SECCOMP_USER_NOTIF_FLAG_CONTINUE as u32,
        };
        assert_eq!(
            unsafe {
                libc::ioctl(
                    listener.as_raw_fd(),
                    libc::SECCOMP_IOCTL_NOTIF_ID_VALID,
                    &id,
                )
            },
            0
        );
        assert_eq!(
            unsafe {
                libc::ioctl(
                    listener.as_raw_fd(),
                    libc::SECCOMP_IOCTL_NOTIF_SEND,
                    &mut response,
                )
            },
            0
        );
    }
    struct NativeFixture {
        path: PathBuf,
        host: WorkspaceHost,
        native: Arc<support::Native>,
    }
    impl NativeFixture {
        fn new() -> Self {
            let native = support::Native::new(support::Gate::None);
            Self::with_delivery(native.clone(), native.delivery())
        }
        fn with_delivery(native: Arc<support::Native>, delivery: OperationDelivery) -> Self {
            let path = PathBuf::from(std::env::var("LAYERFS_EDIT_TEST_ROOT").unwrap());
            let host = WorkspaceHost::new(
                WorkspaceConfig {
                    root: path.clone(),
                    max_count: 3,
                    memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                    disk_budget_bytes: Some(8 * 1024 * 1024),
                },
                delivery,
            )
            .unwrap();
            Self { path, host, native }
        }
        fn retained() -> Self {
            let fixture = Self::new();
            let mount = fixture.path.join("workspace/failed");
            fs::create_dir(&mount).unwrap();
            fs::write(mount.join("unowned"), b"sentinel").unwrap();
            let main_filters = filters(tid());
            std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        let before = filters(tid());
                        assert_eq!(syscall_filter(removal_calls(), false), 0);
                        assert_eq!(filters(tid()), before + 1);
                        assert!(matches!(
                            fixture.host.attach(fixture.options(31), deadline()),
                            Err(WorkspaceError::Io)
                        ));
                    })
                    .join()
                    .unwrap();
            });
            assert_eq!(filters(tid()), main_filters);
            assert!(
                fixture.backing().is_dir(),
                "private backing must have been acquired"
            );
            assert_eq!(fs::read_dir(fixture.backing()).unwrap().count(), 0);
            assert_eq!(
                fixture.failure(),
                AttachmentFailure {
                    cause: WorkspaceError::Io,
                    cleanup: Some(WorkspaceError::Io),
                    progress: AttachmentCleanupProgress::Retained {
                        mount_directory: false,
                        metadata_arena: false,
                        backing_directory: true,
                    },
                }
            );
            {
                let observed = fixture.native.observations.lock().unwrap();
                assert!(matches!(observed.operations.as_slice(), [
                    Operation::HistoryQuery(HistoryQuery::GetBranch { .. }),
                    Operation::Inspect { query: Inspect::Attributes { path }, .. }
                ] if path.is_empty()));
            }
            println!("ATTACHMENT_NATIVE_FAILURE {:?}", fixture.failure());
            fixture
        }
        fn options(&self, incarnation: u8) -> AttachOptions {
            let owner = fs::metadata(&self.path).unwrap();
            AttachOptions {
                access: WorkspaceAccess::ReadOnly,
                id: "failed".into(),
                incarnation: [incarnation; 32],
                store: 1,
                base: Base::Branch(support::hex("LAYERFS_EDIT_BRANCH")),
                owner_uid: owner.uid(),
                owner_gid: owner.gid(),
            }
        }
        fn backing(&self) -> PathBuf {
            self.path.join("private-backing/failed")
        }
        fn failure(&self) -> AttachmentFailure {
            let Attachment::Failed(failure) = self.host.attachment("failed", [31; 32]).unwrap()
            else {
                panic!("failed attachment owner was lost")
            };
            failure
        }
        fn assert_unowned(&self) {
            assert_eq!(
                fs::read(self.path.join("workspace/failed/unowned")).unwrap(),
                b"sentinel"
            );
        }
        fn reattach(&self) {
            self.assert_unowned();
            assert!(!self.backing().exists());
            assert!(matches!(
                self.host.attachment("failed", [31; 32]),
                Err(WorkspaceError::NotFound)
            ));
            fs::remove_file(self.path.join("workspace/failed/unowned")).unwrap();
            fs::remove_dir(self.path.join("workspace/failed")).unwrap();
            self.read_new_attachment();
        }
        fn read_new_attachment(&self) {
            let workspace = self.host.attach(self.options(32), deadline()).unwrap();
            let file = workspace
                .lookup(
                    workspace.root().serial,
                    b"data.bin",
                    ReferenceScope::Local,
                    deadline(),
                )
                .unwrap();
            let handle = workspace.open(file.serial, ReferenceScope::Local).unwrap();
            assert_eq!(
                workspace.read(handle, 0, 128, deadline()).unwrap().as_ref(),
                (0..128).collect::<Vec<u8>>()
            );
            workspace.release(handle).unwrap();
            workspace.forget(file.serial, 1, ReferenceScope::Local);
            workspace.close_clean().unwrap();
            println!("ATTACHMENT_RESOURCE reattached-native-read=PASS clean-close=PASS");
        }
    }

    #[test]
    #[ignore = "requires real Branch/source service, ext4 and thread-local syscall denial"]
    fn attachment_retained() {
        let f = NativeFixture::retained();
        let failure = f.failure();
        assert_eq!(
            f.host
                .cleanup_failed_attach("failed", [31; 32], Instant::now()),
            Err(WorkspaceError::Deadline)
        );
        assert_eq!(f.failure(), failure);
        assert_eq!(
            f.host.cleanup_failed_attach("failed", [32; 32], deadline()),
            Err(WorkspaceError::NotFound)
        );
        assert!(matches!(
            f.host.attach(f.options(32), deadline()),
            Err(WorkspaceError::Busy)
        ));
        let mut other = f.options(31);
        other.id = "other".into();
        assert!(matches!(
            f.host.attach(other, deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(
            f.native.observations.lock().unwrap().operations.len(),
            2,
            "observation/refusal must not replay Attach"
        );
        f.assert_unowned();
        check("retained-failure-exact-identity-expiry-and-no-replay");
        f.host
            .cleanup_failed_attach("failed", [31; 32], deadline())
            .unwrap();
        f.reattach();
        check("explicit-cleanup-preserves-unowned-mount-and-permits-new-attach");
    }

    #[test]
    #[ignore = "requires real Branch/source service, ext4 and thread-local syscall denial"]
    fn attachment_substitution() {
        let f = NativeFixture::retained();
        let acquired = f.path.join("private-backing/acquired");
        fs::rename(f.backing(), &acquired).unwrap();
        fs::create_dir(f.backing()).unwrap();
        let foreign = fs::metadata(f.backing()).unwrap();
        let owned = fs::metadata(&acquired).unwrap();
        assert_ne!((foreign.dev(), foreign.ino()), (owned.dev(), owned.ino()));
        assert_eq!(fs::read_dir(f.backing()).unwrap().count(), 0);
        assert_eq!(
            f.host.cleanup_failed_attach("failed", [31; 32], deadline()),
            Err(WorkspaceError::Io)
        );
        assert_eq!(f.failure().cause, WorkspaceError::Io);
        let preserved = fs::metadata(f.backing()).unwrap();
        assert_eq!(
            (preserved.dev(), preserved.ino()),
            (foreign.dev(), foreign.ino())
        );
        assert!(acquired.is_dir());
        f.assert_unowned();
        check("cleanup-refuses-replaced-backing-identity-and-retains-original-cause");
        fs::remove_dir(f.backing()).unwrap();
        fs::rename(acquired, f.backing()).unwrap();
        f.host
            .cleanup_failed_attach("failed", [31; 32], deadline())
            .unwrap();
        f.reattach();
        check("restored-owned-identity-allows-explicit-cleanup");
    }

    fn cleanup_gate(expire: bool) {
        let f = NativeFixture::retained();
        let (send, receive) = mpsc::sync_channel(1);
        let main_filters = filters(tid());
        std::thread::scope(|scope| {
            let caller = scope.spawn(|| {
                let before = filters(tid());
                let listener =
                    unsafe { OwnedFd::from_raw_fd(syscall_filter(removal_calls(), true)) };
                assert_eq!(filters(tid()), before + 1);
                let end = Instant::now() + Duration::from_secs(if expire { 1 } else { 10 });
                send.send((listener, tid(), end)).unwrap();
                f.host.cleanup_failed_attach("failed", [31; 32], end)
            });
            let (listener, task, end) = receive.recv_timeout(Duration::from_secs(5)).unwrap();
            let event = notification(&listener, end);
            assert_eq!(event.pid, task);
            assert_eq!(event.data.arch, arch());
            assert!(removal_calls().contains(&i64::from(event.data.nr)));
            assert_eq!(f.failure().progress, AttachmentCleanupProgress::Running);
            assert_eq!(
                f.host.cleanup_failed_attach("failed", [31; 32], deadline()),
                Err(WorkspaceError::Busy)
            );
            assert!(matches!(
                f.host.attach(f.options(32), deadline()),
                Err(WorkspaceError::Busy)
            ));
            f.assert_unowned();
            assert!(f.backing().is_dir());
            check("inflight-cleanup-retains-observable-owner-with-immediate-busy-admission");
            if expire {
                assert!(
                    Instant::now() < end,
                    "deadline must expire inside the actual removal syscall"
                );
                while let Some(remaining) = end.checked_duration_since(Instant::now()) {
                    std::thread::park_timeout(remaining);
                }
            }
            continue_call(&listener, event.id);
            let result = caller.join().unwrap();
            if expire {
                assert_eq!(result, Err(WorkspaceError::Deadline));
            } else {
                result.unwrap();
            }
        });
        assert_eq!(filters(tid()), main_filters);
        if expire {
            assert_eq!(
                f.failure(),
                AttachmentFailure {
                    cause: WorkspaceError::Io,
                    cleanup: Some(WorkspaceError::Deadline),
                    progress: AttachmentCleanupProgress::Retained {
                        mount_directory: false,
                        metadata_arena: false,
                        backing_directory: false,
                    },
                }
            );
            assert!(!f.backing().exists());
            f.host
                .cleanup_failed_attach("failed", [31; 32], deadline())
                .unwrap();
            check("deadline-after-removal-retains-original-cause-and-released-resource-progress");
        }
        f.reattach();
        check("one-admitted-cleanup-retires-owner-after-real-kernel-removal");
    }

    #[test]
    #[ignore = "requires real Branch/source service and thread-local seccomp user notification"]
    fn attachment_concurrent_cleanup() {
        cleanup_gate(false);
    }

    #[test]
    #[ignore = "requires real Branch/source service and thread-local seccomp deadline gate"]
    fn attachment_cleanup_deadline() {
        cleanup_gate(true);
    }

    #[test]
    #[ignore = "requires real Branch/source service; external adapter allocation contract subset"]
    fn attachment_branch_capacity() {
        use std::sync::atomic::AtomicBool;
        let native = support::Native::new(support::Gate::None);
        let forwarding = native.clone();
        let inflate = Arc::new(AtomicBool::new(false));
        let enabled = inflate.clone();
        let inflated = Arc::new(AtomicU64::new(0));
        let count = inflated.clone();
        let f = NativeFixture::with_delivery(
            native,
            Arc::new(move |request, input, output, end| {
                let mut response = forwarding.call(request, input, output, end)?;
                if enabled.load(Ordering::Acquire)
                    && matches!(
                        request.operation,
                        Operation::HistoryQuery(HistoryQuery::GetBranch { .. })
                    )
                {
                    let Response::History(history) = &mut response else {
                        panic!("native branch reply missing")
                    };
                    let HistoryResult::BranchSnapshot(snapshot) = history.as_mut() else {
                        panic!("native branch snapshot missing")
                    };
                    let logical_name = snapshot.branch.name.clone();
                    snapshot
                        .branch
                        .name
                        .reserve_exact(DEFAULT_MEMORY_BUDGET_BYTES);
                    assert_eq!(snapshot.branch.name, logical_name);
                    assert!(snapshot.branch.name.capacity() >= DEFAULT_MEMORY_BUDGET_BYTES);
                    count.fetch_add(1, Ordering::Relaxed);
                }
                Ok(response)
            }),
        );
        let mut options = f.options(1);
        options.id = "observer".into();
        let observer = f.host.attach(options, deadline()).unwrap();
        let mut seed = f.options(2);
        seed.id = "seed".into();
        f.host
            .attach(seed, deadline())
            .unwrap()
            .close_clean()
            .unwrap();
        let baseline = observer.status().unwrap().accounted_bytes;
        inflate.store(true, Ordering::Release);
        for incarnation in 100..112 {
            let mut options = f.options(incarnation);
            options.id = "capacity".into();
            options.access = WorkspaceAccess::LocalEdit;
            assert!(matches!(
                f.host.attach(options, deadline()),
                Err(WorkspaceError::Capacity)
            ));
            assert!(matches!(
                f.host.attachment("capacity", [incarnation; 32]),
                Err(WorkspaceError::NotFound)
            ));
            assert!(!f.path.join("workspace/capacity").exists());
            assert!(!f.path.join("private-backing/capacity").exists());
            assert_eq!(observer.status().unwrap().accounted_bytes, baseline);
        }
        assert_eq!(inflated.load(Ordering::Relaxed), 12);
        check("adapter-spare-capacity-fails-before-acquisition-without-twelve-attempt-arena-leak");
        inflate.store(false, Ordering::Release);
        let mut options = f.options(112);
        options.id = "capacity".into();
        options.access = WorkspaceAccess::LocalEdit;
        f.host
            .attach(options, deadline())
            .unwrap()
            .close_clean()
            .unwrap();
        observer.close_clean().unwrap();
        println!("ATTACHMENT_RESOURCE adapter-spare-capacity-attempts=12 stable-accounted-bytes={baseline} later-local-edit-clean-close=PASS wire-oversize-claim=false rss-claim=false");
        check("later-local-edit-attach-and-clean-close-after-twelve-capacity-failures");
    }

    fn stat_path(event: &libc::seccomp_notif) -> Vec<u8> {
        let mut bytes = [0u8; 4096];
        let local = libc::iovec {
            iov_base: bytes.as_mut_ptr().cast(),
            iov_len: bytes.len(),
        };
        let remote = libc::iovec {
            iov_base: event.data.args[1] as *mut _,
            iov_len: bytes.len(),
        };
        // The kernel copies the blocked thread's C string; never dereference its
        // argument pointer or fabricate stat results. Both native ABIs use args[1].
        let read = unsafe { libc::process_vm_readv(libc::getpid(), &local, 1, &remote, 1, 0) };
        assert!(read > 0, "{}", std::io::Error::last_os_error());
        let end = bytes[..read as usize]
            .iter()
            .position(|byte| *byte == 0)
            .unwrap();
        bytes[..end].to_vec()
    }

    #[test]
    #[ignore = "requires native mount-leaf stat deadline gate and retained ownership"]
    fn attachment_mount_substitution() {
        use std::os::unix::ffi::OsStrExt;
        let f = NativeFixture::new();
        let mount = f.path.join("workspace/failed");
        let calls = [libc::SYS_statx, libc::SYS_newfstatat];
        let (send, receive) = mpsc::sync_channel(1);
        let main_filters = filters(tid());
        std::thread::scope(|scope| {
            let caller = scope.spawn(|| {
                let task = tid();
                let before = filters(task);
                let listener = unsafe { OwnedFd::from_raw_fd(syscall_filter(&calls, true)) };
                let end = Instant::now() + Duration::from_secs(3);
                // Hand off before any file inspection: even /proc metadata can
                // use statx and would block this caller on its own listener.
                send.send((listener, task, before, end)).unwrap();
                f.host.attach(f.options(31), end)
            });
            let (listener, task, before, end) =
                receive.recv_timeout(Duration::from_secs(3)).unwrap();
            assert_eq!(filters(task), before + 1);
            let mut reached = false;
            for _ in 0..128 {
                let event = notification(&listener, end);
                assert_eq!(event.pid, task);
                assert_eq!(event.data.arch, arch());
                assert!(calls.contains(&i64::from(event.data.nr)));
                if stat_path(&event) == mount.as_os_str().as_bytes() {
                    assert!(
                        mount.is_dir(),
                        "real mkdir must complete before the stat gate"
                    );
                    assert!(f.backing().is_dir());
                    assert!(matches!(
                        f.host.attachment("failed", [31; 32]),
                        Ok(Attachment::Attaching)
                    ));
                    assert!(Instant::now() < end);
                    while let Some(remaining) = end.checked_duration_since(Instant::now()) {
                        std::thread::park_timeout(remaining);
                    }
                    continue_call(&listener, event.id);
                    reached = true;
                    break;
                }
                continue_call(&listener, event.id);
            }
            assert!(reached, "owned mount leaf stat was not observed");
            assert!(matches!(
                caller.join().unwrap(),
                Err(WorkspaceError::Deadline)
            ));
        });
        assert_eq!(filters(tid()), main_filters);
        assert_eq!(
            f.failure(),
            AttachmentFailure {
                cause: WorkspaceError::Deadline,
                cleanup: Some(WorkspaceError::Deadline),
                progress: AttachmentCleanupProgress::Retained {
                    mount_directory: true,
                    metadata_arena: false,
                    backing_directory: true,
                },
            }
        );
        let acquired = f.path.join("workspace/acquired");
        fs::rename(&mount, &acquired).unwrap();
        fs::create_dir(&mount).unwrap();
        let foreign = fs::metadata(&mount).unwrap();
        let owned = fs::metadata(&acquired).unwrap();
        assert_ne!((foreign.dev(), foreign.ino()), (owned.dev(), owned.ino()));
        assert_eq!(
            f.host.cleanup_failed_attach("failed", [31; 32], deadline()),
            Err(WorkspaceError::Denied)
        );
        assert_eq!(
            f.failure(),
            AttachmentFailure {
                cause: WorkspaceError::Deadline,
                cleanup: Some(WorkspaceError::Denied),
                progress: AttachmentCleanupProgress::Retained {
                    mount_directory: true,
                    metadata_arena: false,
                    backing_directory: false,
                },
            }
        );
        assert!(!f.backing().exists());
        let preserved = fs::metadata(&mount).unwrap();
        assert_eq!(
            (preserved.dev(), preserved.ino()),
            (foreign.dev(), foreign.ino())
        );
        assert!(acquired.is_dir());
        println!(
            "ATTACHMENT_NATIVE_FAILURE owned-leaf={} foreign-leaf={} {:?}",
            owned.ino(),
            foreign.ino(),
            f.failure()
        );
        check("owned-mount-leaf-substitution-refused-with-progress-and-original-deadline-cause");
        fs::remove_dir(&mount).unwrap();
        fs::rename(acquired, &mount).unwrap();
        f.host
            .cleanup_failed_attach("failed", [31; 32], deadline())
            .unwrap();
        assert!(!mount.exists());
        assert!(matches!(
            f.host.attachment("failed", [31; 32]),
            Err(WorkspaceError::NotFound)
        ));
        f.read_new_attachment();
        check("restored-owned-mount-leaf-cleans-and-releases-exact-attachment");
    }
}
