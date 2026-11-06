//! Advisory file locks and this process's ids through `nix`, with no unsafe
//! code: the discovery and unmount locks (`flock`) and the native command
//! identity (`geteuid`, `getegid`).
use std::fs::File;
use std::os::unix::io::AsRawFd;

/// Takes the lock without waiting: `Ok(true)` held, `Ok(false)` another
/// owner holds it, `Err` any other failure.
#[allow(deprecated)] // nix's `Flock` consumes the file; the locks here keep it.
pub(crate) fn try_lock(file: &File, exclusive: bool) -> std::io::Result<bool> {
    use nix::fcntl::{flock, FlockArg};
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
#[allow(deprecated)]
pub(crate) fn unlock(file: &File) {
    let _ = nix::fcntl::flock(file.as_raw_fd(), nix::fcntl::FlockArg::Unlock);
}

/// This process's effective uid.
pub(crate) fn euid() -> u32 {
    nix::unistd::geteuid().as_raw()
}

/// This process's effective gid.
pub(crate) fn egid() -> u32 {
    nix::unistd::getegid().as_raw()
}
