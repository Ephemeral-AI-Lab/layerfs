//! Advisory file locks and this process's ids: the discovery and unmount
//! locks and the native command identity. Unix keeps nix and no unsafe
//! code; Windows locks the whole file with LockFileEx ranges on the same
//! dedicated lock files, and ids stay unix-only there.
#[cfg(unix)]
use std::fs::File;

/// Takes the lock without waiting: Ok(true) held, Ok(false) another
/// owner holds it, Err any other failure.
#[cfg(unix)]
#[allow(deprecated)] // nix's Flock consumes the file; the locks here keep it.
pub(crate) fn try_lock(file: &File, exclusive: bool) -> std::io::Result<bool> {
    use nix::fcntl::{flock, FlockArg};
    use std::os::unix::io::AsRawFd;
    let arg = if exclusive {
        FlockArg::LockExclusiveNonblock
    } else {
        FlockArg::LockSharedNonblock
    };
    match flock(file.as_raw_fd(), arg) {
        Ok(()) => Ok(true),
        Err(nix::errno::Errno::EWOULDBLOCK) => Ok(false),
        Err(errno) => Err(std::io::Error::from(errno)),
    }
}

/// Releases the lock explicitly: a child forked meanwhile shares the open
/// file description until its exec, so closing this descriptor alone would
/// leave it held (F-053).
#[cfg(unix)]
#[allow(deprecated)]
pub(crate) fn unlock(file: &File) {
    use std::os::unix::io::AsRawFd;
    let _ = nix::fcntl::flock(file.as_raw_fd(), nix::fcntl::FlockArg::Unlock);
}

/// This process's effective uid.
#[cfg(unix)]
pub(crate) fn euid() -> u32 {
    nix::unistd::geteuid().as_raw()
}

/// This process's effective gid.
#[cfg(unix)]
pub(crate) fn egid() -> u32 {
    nix::unistd::getegid().as_raw()
}

/// The crate's one audited Win32 lock surface (the V1 line kept the same
/// shape in its store-service): whole-file advisory ranges with the
/// fail-immediately flag, matching flock's non-blocking take. The lock
/// files carry no content I/O, so range locks cannot wedge them. Upgrade
/// path once more than locks is needed: a windows-sys dependency module.
#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) mod windows {
    use std::fs::File;
    use std::io;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct OVERLAPPED {
        Internal: usize,
        InternalHigh: usize,
        Offset: u32,
        OffsetHigh: u32,
        hEvent: usize,
    }

    extern "system" {
        #[link_name = "LockFileEx"]
        fn LockFileEx(
            file: usize,
            flags: u32,
            reserved: u32,
            len_low: u32,
            len_high: u32,
            overlapped: *mut OVERLAPPED,
        ) -> u32;
        #[link_name = "UnlockFileEx"]
        fn UnlockFileEx(
            file: usize,
            reserved: u32,
            len_low: u32,
            len_high: u32,
            overlapped: *mut OVERLAPPED,
        ) -> u32;
    }

    const LOCKFILE_FAIL_IMMEDIATELY: u32 = 0x0000_0001;
    const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x0000_0002;
    const ERROR_LOCK_VIOLATION: i32 = 33;

    /// Ok(true) held, Ok(false) another handle holds the range.
    pub(crate) fn try_lock(file: &File, exclusive: bool) -> io::Result<bool> {
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        let mut flags = LOCKFILE_FAIL_IMMEDIATELY;
        if exclusive {
            flags |= LOCKFILE_EXCLUSIVE_LOCK;
        }
        let taken = unsafe {
            LockFileEx(
                file.as_raw_handle() as usize,
                flags,
                0,
                u32::MAX,
                u32::MAX,
                &mut overlapped,
            )
        };
        if taken != 0 {
            return Ok(true);
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_LOCK_VIOLATION) {
            Ok(false)
        } else {
            Err(error)
        }
    }

    /// Releases the range this handle locked; a closed handle frees it too.
    pub(crate) fn unlock(file: &File) {
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        let _ = unsafe {
            UnlockFileEx(file.as_raw_handle() as usize, 0, u32::MAX, u32::MAX, &mut overlapped)
        };
    }
}

#[cfg(windows)]
pub(crate) use windows::{try_lock, unlock};
