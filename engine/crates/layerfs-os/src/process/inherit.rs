//! Keeping a long-lived child from inheriting this process's descriptors
//! (#833): a descriptor this process was given without close-on-exec (a
//! pipe its own caller passed, a jobserver, an IPC channel) stays open in
//! a child that outlives it, so whoever waits for that descriptor's end
//! waits for the child. [`close_inherited`] marks every descriptor above
//! stderr close-on-exec in the child, between its fork and its exec, so its
//! program starts with descriptors 0-2 only. Marking, not closing, keeps
//! the standard library's own exec-error pipe working until the exec.
//! This crate's second audited `unsafe` file.
use nix::libc;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

/// Marks every descriptor above 2 close-on-exec in `command`'s child. On
/// Linux one `close_range(3, ~0, CLOSE_RANGE_CLOEXEC)` does it; where that
/// is refused (a kernel before 5.11), and on macOS, the descriptors this
/// process has open, listed before the fork, are marked one by one.
/// Descriptors opened after the listing are the standard library's, which
/// are close-on-exec already. The child is then forked rather than spawned
/// by `posix_spawn`.
#[allow(unsafe_code)]
pub fn close_inherited(command: &mut Command) -> io::Result<()> {
    let listed = open_descriptors()?;
    // SAFETY: the closure runs in the child between fork and exec, where
    // only async-signal-safe calls are sound. It makes `close_range` and
    // `fcntl` system calls over a list built before the fork, and neither
    // allocates, takes a lock nor touches another thread's state.
    unsafe {
        command.pre_exec(move || {
            mark(&listed);
            Ok(())
        });
    }
    Ok(())
}

/// This process's descriptors above 2, from the kernel's listing. On Linux
/// a listing that cannot be read is empty (`close_range` covers it).
fn open_descriptors() -> io::Result<Vec<libc::c_int>> {
    #[cfg(target_os = "linux")]
    let entries = match std::fs::read_dir("/proc/self/fd") {
        Ok(entries) => entries,
        Err(_) => return Ok(Vec::new()),
    };
    #[cfg(not(target_os = "linux"))]
    let entries = std::fs::read_dir("/dev/fd")?;
    let mut found = Vec::new();
    for entry in entries {
        let name = entry?.file_name();
        if let Some(fd) = name.to_str().and_then(|n| n.parse::<libc::c_int>().ok()) {
            if fd > 2 {
                found.push(fd);
            }
        }
    }
    Ok(found)
}

/// In the child: marks descriptors above 2 close-on-exec.
#[allow(unsafe_code)]
fn mark(listed: &[libc::c_int]) {
    #[cfg(target_os = "linux")]
    {
        /// `CLOSE_RANGE_CLOEXEC` from `<linux/close_range.h>` (Linux 5.11).
        const CLOSE_RANGE_CLOEXEC: libc::c_uint = 1 << 2;
        // SAFETY: a plain system call on this process's descriptor table,
        // with no pointer argument.
        let marked = unsafe {
            libc::syscall(
                libc::SYS_close_range,
                3 as libc::c_uint,
                libc::c_uint::MAX,
                CLOSE_RANGE_CLOEXEC,
            )
        };
        if marked == 0 {
            return;
        }
    }
    for &fd in listed {
        // SAFETY: `fcntl` with an integer argument on a descriptor number;
        // one closed since the listing gives EBADF, which is ignored.
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags >= 0 && flags & libc::FD_CLOEXEC == 0 {
                libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
            }
        }
    }
}
