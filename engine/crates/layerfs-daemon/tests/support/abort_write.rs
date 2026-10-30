//! Kernel-denied writes to this test's own, already-bound abort descriptor.
//! The filter belongs only to the calling thread, which exits after Unmount.
//! No descriptor is replaced and no test code writes a fusectl file.
use layerfs_bridge::control::ReadyMount;
use nix::libc;
use std::{
    fs, io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::PathBuf,
};

pub fn descriptor(ready: &ReadyMount) -> u32 {
    let own = super::fusectl::own(ready);
    let path = PathBuf::from(super::fusectl::CONNECTIONS)
        .join(own.minor.to_string())
        .join("abort");
    let descriptors: Vec<_> = fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| fs::read_link(entry.path()).is_ok_and(|target| target == path))
        .map(|entry| entry.file_name().to_string_lossy().parse().unwrap())
        .collect();
    assert_eq!(descriptors.len(), 1, "one bound control for this mount");
    descriptors[0]
}

/// errno=0 makes write return zero bytes; EIO preserves a concrete failure.
pub fn deny_write(fd: u32, errno: u32) {
    install(fd, libc::SECCOMP_RET_ERRNO | errno, false);
}

/// Intercept the one write so the test can acknowledge it without ending the
/// connection, staging a second Join stop after an apparently accepted abort.
pub fn intercept_write(fd: u32) -> OwnedFd {
    let listener = install(fd, libc::SECCOMP_RET_USER_NOTIF, true);
    // SAFETY: a successful NEW_LISTENER call returns a new owned descriptor.
    unsafe { OwnedFd::from_raw_fd(listener) }
}

fn install(fd: u32, action: u32, notify: bool) -> i32 {
    let instruction = |code, jt, jf, k| libc::sock_filter { code, jt, jf, k };
    let load = (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16;
    let equal = (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16;
    let ret = (libc::BPF_RET | libc::BPF_K) as u16;
    let mut instructions = [
        instruction(
            load,
            0,
            0,
            std::mem::offset_of!(libc::seccomp_data, nr) as u32,
        ),
        instruction(equal, 0, 3, libc::SYS_write as u32),
        instruction(
            load,
            0,
            0,
            std::mem::offset_of!(libc::seccomp_data, args) as u32,
        ),
        instruction(equal, 0, 1, fd),
        instruction(ret, 0, 0, action),
        instruction(ret, 0, 0, libc::SECCOMP_RET_ALLOW),
    ];
    let program = libc::sock_fprog {
        len: instructions.len() as u16,
        filter: instructions.as_mut_ptr(),
    };
    // SAFETY: the kernel reads the live, correctly sized filter in this call.
    // No TSYNC: other test/product threads retain their syscall permissions.
    unsafe {
        assert_eq!(libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0), 0);
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
        assert!(
            result >= 0,
            "install filter: {}",
            io::Error::last_os_error()
        );
        result as i32
    }
}

pub fn acknowledge_write(listener: OwnedFd, expected_fd: u32) {
    // SAFETY: all-zero notification structures are valid ioctl output/input
    // buffers. Poll is bounded; this listener has exactly one consumer.
    unsafe {
        let fd = listener.as_raw_fd();
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(
            libc::poll(&mut poll, 1, 10_000),
            1,
            "abort notification deadline"
        );
        let mut request: libc::seccomp_notif = std::mem::zeroed();
        assert_eq!(
            libc::ioctl(fd, libc::SECCOMP_IOCTL_NOTIF_RECV, &mut request),
            0
        );
        assert_eq!(request.data.nr, libc::SYS_write as i32);
        assert_eq!(request.data.args[0], u64::from(expected_fd));
        assert_eq!(request.data.args[2], 1);
        let reply = libc::seccomp_notif_resp {
            id: request.id,
            val: 1,
            error: 0,
            flags: 0,
        };
        assert_eq!(libc::ioctl(fd, libc::SECCOMP_IOCTL_NOTIF_SEND, &reply), 0);
    }
}
