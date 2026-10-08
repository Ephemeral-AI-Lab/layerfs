//! Native kernel faults outside product source; ownership survives each entered error.
#[cfg(target_os = "linux")]
#[path = "../../layerfs-workspace/tests/support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::{CommitOutcomeWire, Root};
    use layerfs_fuse::{MountError, MountFailure, MountHandle, MountPhase};
    use layerfs_workspace::*;
    use std::{
        fs::{self, File, OpenOptions},
        io,
        os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::fs::{FileExt, FileTypeExt, MetadataExt},
        },
        path::Path,
        sync::mpsc,
        time::{Duration, Instant},
    };

    fn check(id: &str) {
        println!("MOUNT_FAILURE_CHECK {id} PASS");
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
    fn mount_row(path: &Path) -> Option<String> {
        fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .find(|line| line.split(' ').nth(4) == path.to_str())
            .map(|line| {
                let (identity, filesystem) = line.split_once(" - ").unwrap();
                assert!(identity.split(' ').next().unwrap().parse::<u64>().unwrap() > 0);
                assert!(filesystem.starts_with("fuse layerfs "), "{line}");
                line.to_string()
            })
    }
    fn failed(result: Result<MountHandle, Box<MountFailure>>) -> Box<MountFailure> {
        match result {
            Err(failure) => failure,
            Ok(_) => panic!("faulted mount unexpectedly succeeded"),
        }
    }
    fn cleanup(f: &Fixture, failure: &mut MountFailure, expected: Option<&str>) {
        assert_eq!(mount_row(f.workspace.mount_path()).as_deref(), expected);
        let state = f.workspace.status().unwrap();
        assert!(state.mounted && state.stopping && !state.closed);
        assert_eq!(f.workspace.close_clean(), Err(WorkspaceError::Busy));
        let mut owner = failure
            .retained
            .take()
            .expect("entered error must retain its owner");
        // A separate explicit cleanup operation gets its own declared deadline.
        owner.unmount(deadline()).unwrap();
        assert!(mount_row(f.workspace.mount_path()).is_none());
        assert!(!f.workspace.status().unwrap().mounted);
        f.workspace.close_clean().unwrap();
        println!(
            "MOUNT_FAILURE_RETAINED phase={:?} cause={} cleanup=PASS",
            failure.phase, failure.cause
        );
    }
    fn stmt(code: u16, k: u32) -> libc::sock_filter {
        libc::sock_filter {
            code,
            jt: 0,
            jf: 0,
            k,
        }
    }
    fn jump(k: u32, jf: u8) -> libc::sock_filter {
        libc::sock_filter {
            code: 0x15,
            jt: 0,
            jf,
            k,
        }
    }
    fn arch() -> u32 {
        assert!(cfg!(target_endian = "little"));
        if cfg!(target_arch = "aarch64") {
            0xc00000b7
        } else if cfg!(target_arch = "x86_64") {
            0xc000003e
        } else {
            panic!("unsupported native fault architecture")
        }
    }
    fn install(code: &mut [libc::sock_filter], listener: bool) -> i32 {
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
                0,
                "{}",
                io::Error::last_os_error()
            );
            // Never TSYNC: pre-existing observer/cleanup threads stay unfiltered.
            let flags = if listener {
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
            assert!(result >= 0, "{}", io::Error::last_os_error());
            result.try_into().unwrap()
        }
    }
    fn deny(syscalls: &[i64], error: i32) {
        let mut code = vec![stmt(0x20, 4), jump(arch(), 0), stmt(0x20, 0)];
        // A foreign ABI passes untouched; these tests only claim the native ABI.
        code[1].jf = (2 * syscalls.len() + 1).try_into().unwrap();
        for syscall in syscalls {
            code.push(jump((*syscall).try_into().unwrap(), 1));
            code.push(stmt(0x06, libc::SECCOMP_RET_ERRNO | error as u32));
        }
        code.push(stmt(0x06, libc::SECCOMP_RET_ALLOW));
        assert_eq!(install(&mut code, false), 0);
    }
    fn notify_writev() -> OwnedFd {
        let mut code = [
            stmt(0x20, 4),
            jump(arch(), 3),
            stmt(0x20, 0),
            jump(libc::SYS_writev as u32, 1),
            stmt(0x06, libc::SECCOMP_RET_USER_NOTIF),
            stmt(0x06, libc::SECCOMP_RET_ALLOW),
        ];
        unsafe { OwnedFd::from_raw_fd(install(&mut code, true)) }
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
        let remaining = end
            .checked_duration_since(Instant::now())
            .expect("notification arrived too late");
        let mut poll = libc::pollfd {
            fd: listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
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
            0,
            "{}",
            io::Error::last_os_error()
        );
        event
    }
    fn verify_init(event: &libc::seccomp_notif, task: u32) {
        assert_eq!(event.pid, task);
        assert_eq!(event.data.arch, arch());
        assert_eq!(i64::from(event.data.nr), libc::SYS_writev);
        let fd: i32 = event.data.args[0].try_into().unwrap();
        assert_eq!(
            fs::read_link(format!("/proc/self/fd/{fd}")).unwrap(),
            Path::new("/dev/fuse")
        );
        let descriptor = fs::metadata(format!("/proc/self/fd/{fd}")).unwrap();
        assert!(descriptor.file_type().is_char_device());
        assert_eq!(descriptor.rdev(), fs::metadata("/dev/fuse").unwrap().rdev());
        let count: usize = event.data.args[2].try_into().unwrap();
        assert!((1..=16).contains(&count));
        let mut remote = vec![
            libc::iovec {
                iov_base: std::ptr::null_mut(),
                iov_len: 0
            };
            count
        ];
        let descriptor_bytes = count * std::mem::size_of::<libc::iovec>();
        let local = libc::iovec {
            iov_base: remote.as_mut_ptr().cast(),
            iov_len: descriptor_bytes,
        };
        let descriptors = libc::iovec {
            iov_base: event.data.args[1] as *mut _,
            iov_len: descriptor_bytes,
        };
        // Read through the kernel from the blocked caller; never dereference its
        // raw pointers or fabricate a reply. The actual writev later CONTINUEs.
        assert_eq!(
            unsafe { libc::process_vm_readv(libc::getpid(), &local, 1, &descriptors, 1, 0) },
            descriptor_bytes as isize
        );
        let total: usize = remote.iter().map(|v| v.iov_len).sum();
        let mut remaining = 24;
        for slice in &mut remote {
            slice.iov_len = slice.iov_len.min(remaining);
            remaining -= slice.iov_len;
        }
        assert_eq!(remaining, 0);
        let mut reply = [0u8; 24];
        let local = libc::iovec {
            iov_base: reply.as_mut_ptr().cast(),
            iov_len: reply.len(),
        };
        assert_eq!(
            unsafe {
                libc::process_vm_readv(
                    libc::getpid(),
                    &local,
                    1,
                    remote.as_ptr(),
                    remote.len() as _,
                    0,
                )
            },
            24
        );
        assert_eq!(
            u32::from_ne_bytes(reply[..4].try_into().unwrap()) as usize,
            total
        );
        assert_eq!(i32::from_ne_bytes(reply[4..8].try_into().unwrap()), 0);
        assert!(u64::from_ne_bytes(reply[8..16].try_into().unwrap()) > 0);
        assert_eq!(u32::from_ne_bytes(reply[16..20].try_into().unwrap()), 7);
        assert!(u32::from_ne_bytes(reply[20..24].try_into().unwrap()) >= 6);
        println!(
            "MOUNT_INIT_OBSERVED tid={task} fd={fd} reply_bytes={total} unique={}",
            u64::from_ne_bytes(reply[8..16].try_into().unwrap())
        );
    }
    fn continue_call(listener: &OwnedFd, id: u64) {
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
                    libc::SECCOMP_IOCTL_NOTIF_SEND,
                    &mut response,
                )
            },
            0,
            "{}",
            io::Error::last_os_error()
        );
    }

    #[test]
    #[ignore = "requires real FUSE INIT and thread-local seccomp user notification"]
    fn mount_failure_deadline() {
        let f = Fixture::new(Gate::None);
        assert!(mount_row(f.workspace.mount_path()).is_none());
        let main_filters = filters(tid());
        let (send, receive) = mpsc::sync_channel(1);
        let (mut failure, identity) = std::thread::scope(|scope| {
            let caller = scope.spawn(|| {
                let task = tid();
                let before = filters(task);
                let listener = notify_writev();
                assert_eq!(filters(task), before + 1);
                let end = Instant::now() + Duration::from_secs(1);
                send.send((listener, task, end)).unwrap();
                failed(layerfs_fuse::mount(&f.workspace, end))
            });
            let (listener, task, end) = receive.recv_timeout(Duration::from_secs(3)).unwrap();
            let event = notification(&listener, end);
            verify_init(&event, task);
            let identity =
                mount_row(f.workspace.mount_path()).expect("kernel mount must already exist");
            assert!(
                Instant::now() < end,
                "deadline must expire while actual INIT is held"
            );
            println!("MOUNT_IDENTITY {identity}");
            while let Some(remaining) = end.checked_duration_since(Instant::now()) {
                std::thread::park_timeout(remaining);
            }
            continue_call(&listener, event.id);
            let failure = caller.join().unwrap();
            assert!(Instant::now() >= end);
            (failure, identity)
        });
        assert_eq!(filters(tid()), main_filters);
        assert_eq!(failure.phase, MountPhase::Deadline);
        assert!(matches!(&failure.cause, MountError::Deadline));
        cleanup(&f, &mut failure, Some(&identity));
        assert_eq!(failure.phase, MountPhase::Deadline);
        assert!(matches!(&failure.cause, MountError::Deadline));
        check("actual-INIT-deadline-retains-mount-until-explicit-cleanup");
    }

    #[test]
    #[ignore = "requires real initialized mount and native pthread creation refusal"]
    fn mount_failure_worker() {
        let f = Fixture::new(Gate::None);
        let main_filters = filters(tid());
        let mut failure = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let before = filters(tid());
                    deny(&[libc::SYS_clone3, libc::SYS_clone], libc::EAGAIN);
                    assert_eq!(filters(tid()), before + 1);
                    failed(layerfs_fuse::mount(&f.workspace, deadline()))
                })
                .join()
                .unwrap()
        });
        assert_eq!(filters(tid()), main_filters);
        assert_eq!(failure.phase, MountPhase::Worker);
        let MountError::Io(error) = &failure.cause else {
            panic!("{}", failure.cause)
        };
        assert_eq!(error.raw_os_error(), Some(libc::EAGAIN));
        let identity =
            mount_row(f.workspace.mount_path()).expect("initialized session must retain its mount");
        println!("MOUNT_IDENTITY {identity}");
        cleanup(&f, &mut failure, Some(&identity));
        check("native-worker-spawn-failure-retains-initialized-session");
    }

    #[test]
    #[ignore = "requires native mount syscall error with retained reservation"]
    fn mount_failure_session() {
        let f = Fixture::new(Gate::None);
        let main_filters = filters(tid());
        let mut failure = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let before = filters(tid());
                    // EACCES is deliberate: fuser's EPERM path invokes a mount helper.
                    deny(&[libc::SYS_mount], libc::EACCES);
                    assert_eq!(filters(tid()), before + 1);
                    failed(layerfs_fuse::mount(&f.workspace, deadline()))
                })
                .join()
                .unwrap()
        });
        assert_eq!(filters(tid()), main_filters);
        assert_eq!(failure.phase, MountPhase::Session);
        let MountError::Io(error) = &failure.cause else {
            panic!("{}", failure.cause)
        };
        assert!(error.to_string().contains("EACCES"), "{error}");
        cleanup(&f, &mut failure, None);
        check("native-mount-syscall-failure-retains-lease-until-checked-absence");
    }

    #[test]
    #[ignore = "native public admission subset, without a kernel mount"]
    fn mount_failure_admission() {
        let f = Fixture::new(Gate::None);
        let failure = failed(layerfs_fuse::mount(
            &f.workspace,
            Instant::now() - Duration::from_secs(1),
        ));
        assert_eq!(failure.phase, MountPhase::Admission);
        assert!(matches!(&failure.cause, MountError::Deadline));
        assert!(failure.retained.is_none());
        assert!(!f.workspace.status().unwrap().mounted);
        let ro = f
            .host
            .attach(
                Fixture::options("readonly", 32, WorkspaceAccess::ReadOnly),
                deadline(),
            )
            .unwrap();
        let failure = failed(layerfs_fuse::mount_writable(&ro, deadline()));
        assert_eq!(failure.phase, MountPhase::Admission);
        assert!(matches!(
            &failure.cause,
            MountError::Workspace(WorkspaceError::ReadOnly)
        ));
        assert!(failure.retained.is_none());
        assert!(!ro.status().unwrap().mounted);
        let mut options = Fixture::options("foreign", 33, WorkspaceAccess::ReadOnly);
        options.owner_uid = 1001;
        let accounted = f.workspace.status().unwrap().accounted_bytes;
        // Workspace validates directory ownership before FUSE admission is possible.
        assert!(matches!(
            f.host.attach(options, deadline()),
            Err(WorkspaceError::Unsupported)
        ));
        assert_eq!(f.workspace.status().unwrap().accounted_bytes, accounted);
        assert!(!f
            .workspace
            .mount_path()
            .parent()
            .unwrap()
            .join("foreign")
            .exists());
        for workspace in [&f.workspace, &ro] {
            assert!(mount_row(workspace.mount_path()).is_none());
            workspace.close_clean().unwrap();
        }
        check("pre-admission-deadline-and-authority-refusals-retain-no-owner");
    }

    fn read(file: &File) -> [u8; 4] {
        let mut bytes = [0; 4];
        file.read_exact_at(&mut bytes, 0).unwrap();
        bytes
    }
    fn commit_root(outcome: CommitOutcomeWire) -> Root {
        match outcome {
            CommitOutcomeWire::Committed(value) => value.root,
            other => panic!("{other:?}"),
        }
    }
    #[test]
    #[ignore = "requires real readonly then writable mounts and native Commit"]
    fn mount_failure_success() {
        let f = Fixture::new(Gate::None);
        let mut mount = layerfs_fuse::mount(&f.workspace, deadline()).unwrap();
        assert!(mount_row(f.workspace.mount_path()).is_some());
        let path = f.workspace.mount_path().join("data.bin");
        let file = File::open(&path).unwrap();
        assert_eq!(read(&file), [0, 1, 2, 3]);
        assert_eq!(
            OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap_err()
                .raw_os_error(),
            Some(libc::EROFS)
        );
        drop(file);
        mount.unmount(deadline()).unwrap();
        assert!(mount_row(f.workspace.mount_path()).is_none());
        let mut mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        assert!(mount_row(f.workspace.mount_path()).is_some());
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.write_all_at(b"LIVE", 0).unwrap();
        assert_eq!(read(&file), *b"LIVE");
        let root = commit_root(f.workspace.commit(deadline()).unwrap().outcome);
        let saved = attr(f.native.attributes(root, b"data.bin"));
        assert_eq!(f.native.bytes(saved.1, 0, 4), b"LIVE");
        drop(file);
        mount.unmount(deadline()).unwrap();
        assert!(mount_row(f.workspace.mount_path()).is_none());
        f.workspace.close_clean().unwrap();
        check("ordinary-readonly-and-writable-mounts-retain-checked-lifecycle");
    }
}
