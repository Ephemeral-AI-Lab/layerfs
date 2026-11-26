//! Local Unix sockets, using the platform's peer credentials.
use nix::errno::Errno;
use nix::fcntl::{fcntl, FcntlArg, FdFlag, OFlag};
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use nix::sys::socket::{self, AddressFamily, SockFlag, SockType, UnixAddr};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Instant;

/// The credential reported by the kernel for the other end of this socket.
pub fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    #[cfg(target_os = "macos")]
    let uid = socket::getsockopt(stream, socket::sockopt::LocalPeerCred);
    #[cfg(target_os = "linux")]
    let uid = socket::getsockopt(stream, socket::sockopt::PeerCredentials);
    uid.map(|peer| peer.uid()).map_err(io::Error::from)
}

/// Whether Unix can represent this pathname, including its terminator.
pub fn path_fits(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let path_bytes = path.as_os_str().as_bytes();
    !path_bytes.contains(&0) && UnixAddr::new(path).is_ok()
}

/// Connects without blocking on a full Unix listen queue. On Linux a full
/// queue can return WouldBlock, with no pending connection.
pub fn connect(path: &Path, deadline: Instant) -> io::Result<UnixStream> {
    let address = UnixAddr::new(path)?;
    let fd = socket::socket(
        AddressFamily::Unix,
        SockType::Stream,
        SockFlag::empty(),
        None,
    )?;
    fcntl(&fd, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))?;
    fcntl(&fd, FcntlArg::F_SETFL(OFlag::O_NONBLOCK))?;
    match socket::connect(fd.as_raw_fd(), &address) {
        Ok(()) => {}
        Err(Errno::EINPROGRESS) => {
            let timeout = deadline.saturating_duration_since(Instant::now());
            if timeout.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let timeout = PollTimeout::try_from(timeout).unwrap_or(PollTimeout::MAX);
            let mut descriptors = [PollFd::new(fd.as_fd(), PollFlags::POLLOUT)];
            if poll(&mut descriptors, timeout)? == 0 {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let socket_error = socket::getsockopt(&fd, socket::sockopt::SocketError)?;
            if socket_error != 0 {
                return Err(io::Error::from_raw_os_error(socket_error));
            }
        }
        Err(error) => return Err(error.into()),
    }
    let stream = UnixStream::from(fd);
    stream.set_nonblocking(false)?;
    Ok(stream)
}

/// Opens an owner-only regular file, without following a symlink or
/// blocking on a non-regular file used in its place.
pub fn private_file(path: &Path, append: bool) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(!append)
        .write(true)
        .append(append)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != super::flock::euid()
        || metadata.mode() & 0o077 != 0
        || metadata.nlink() != 1
    {
        return Err(io::Error::other(
            "expected a private, owner-only regular file",
        ));
    }
    Ok(file)
}

/// Changes socket mode in the held directory.
pub fn set_socket_private(directory: &File, name: &std::ffi::OsStr) -> io::Result<()> {
    use nix::sys::stat::{fchmodat, FchmodatFlags, Mode};
    fchmodat(
        directory,
        name,
        Mode::S_IRUSR | Mode::S_IWUSR,
        FchmodatFlags::FollowSymlink,
    )
    .map_err(io::Error::from)
}

/// Removes this user's socket relative to the held directory, so a move
/// of the home cannot redirect cleanup into a replacement directory.
pub fn remove_socket(directory: &File, name: &std::ffi::OsStr) -> io::Result<()> {
    use nix::fcntl::AtFlags;
    use nix::sys::stat::{fstatat, SFlag};
    use nix::unistd::{unlinkat, UnlinkatFlags};
    let entry = match fstatat(directory, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(entry) => entry,
        Err(Errno::ENOENT) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if entry.st_uid != super::flock::euid()
        || SFlag::from_bits_truncate(entry.st_mode) & SFlag::S_IFMT != SFlag::S_IFSOCK
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "expected an owner-controlled socket",
        ));
    }
    unlinkat(directory, name, UnlinkatFlags::NoRemoveDir).map_err(io::Error::from)
}
