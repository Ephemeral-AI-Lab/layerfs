//! Safe Darwin file calls with checked close and explicit full synchronization.
use super::connection::SqlWork;
use crate::backend::records::BackendError;
use nix::{
    fcntl::{fcntl, openat, AtFlags, FcntlArg, OFlag},
    sys::stat::{fstatat, Mode},
};
use std::{cell::RefCell, fs::File, os::unix::fs::MetadataExt, time::Instant};
pub(crate) fn filesystem(e: std::io::Error) -> BackendError {
    BackendError::Filesystem(e.kind())
}
pub(crate) fn errno(e: nix::errno::Errno) -> BackendError {
    filesystem(std::io::Error::from_raw_os_error(e as i32))
}
pub(crate) fn open(directory: &File, name: &str, create: bool) -> Result<File, BackendError> {
    let flags = if create {
        OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL
    } else {
        OFlag::O_RDONLY
    };
    openat(
        directory,
        name,
        flags | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::S_IRUSR | Mode::S_IWUSR,
    )
    .map(File::from)
    .map_err(errno)
}
pub(crate) fn custody(
    directory: &File,
    name: &str,
    file: &File,
    device: i64,
    inode: i64,
    length: usize,
) -> Result<(), BackendError> {
    let m = file.metadata().map_err(filesystem)?;
    let named = fstatat(directory, name, AtFlags::AT_SYMLINK_NOFOLLOW).map_err(errno)?;
    if !m.is_file()
        || m.nlink() != 1
        || m.dev() != device as u64
        || m.ino() != inode as u64
        || m.len() != length as u64
        || named.st_dev as u64 != m.dev()
        || named.st_ino != m.ino()
        || named.st_nlink != 1
        || named.st_mode & nix::libc::S_IFMT != nix::libc::S_IFREG
    {
        return Err(BackendError::Integrity);
    }
    Ok(())
}
pub(crate) fn synchronize(
    file: &File,
    directory: bool,
    work: &RefCell<SqlWork>,
) -> Result<(), BackendError> {
    let start = Instant::now();
    {
        let mut w = work.borrow_mut();
        w.segment_full_sync_calls += 1;
        if directory {
            w.segment_directory_sync_calls += 1;
        } else {
            w.segment_file_sync_calls += 1;
        }
    }
    // No weaker fsync substitute or second attempt if full synchronization fails.
    let result = fcntl(file, FcntlArg::F_FULLFSYNC).map_err(|_| BackendError::Unknown);
    work.borrow_mut().segment_sync_ns += start.elapsed().as_nanos() as u64;
    result.map(|_| ())
}
pub(crate) fn stage_directory(file: &File, work: &RefCell<SqlWork>) -> Result<(), BackendError> {
    let start = Instant::now();
    work.borrow_mut().segment_directory_sync_calls += 1;
    // Stage directory metadata first. A same-device F_FULLFSYNC follows before
    // catalogue publication, draining these already submitted metadata writes.
    let result = nix::unistd::fsync(file).map_err(|_| BackendError::Unknown);
    work.borrow_mut().segment_sync_ns += start.elapsed().as_nanos() as u64;
    result
}
pub(crate) fn finish<T>(
    result: Result<T, BackendError>,
    file: File,
    work: &RefCell<SqlWork>,
) -> Result<T, BackendError> {
    work.borrow_mut().segment_close_calls += 1;
    let closed = nix::unistd::close(file).map_err(|_| BackendError::Unknown);
    if result.as_ref().err() == Some(&BackendError::Unknown) || closed.is_err() {
        return Err(BackendError::Unknown);
    }
    closed?;
    result
}
