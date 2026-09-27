//! Kernel open flags accepted by the mounted projection.
use fuser::{Errno, OpenFlags};
use layerfs_workspace::{FileAccess, FileOpenOptions};

// Linux do_open_execat carries __FMODE_EXEC in file flags through FUSE_OPEN.
pub(crate) const KERNEL_FMODE_EXEC: i32 = 1 << 5;

// FUSE forwards kernel UAPI flags. glibc's 64-bit O_LARGEFILE is zero even
// though the kernel sets this bit in every ordinary file description.
#[cfg(target_arch = "aarch64")]
const KERNEL_O_LARGEFILE: i32 = 0o400000;
#[cfg(target_arch = "x86_64")]
const KERNEL_O_LARGEFILE: i32 = 0o100000;
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
const KERNEL_O_LARGEFILE: i32 = 0;

pub(crate) fn flags(
    value: OpenFlags,
    directory: bool,
    writable: bool,
) -> Result<FileOpenOptions, Errno> {
    let mutations = libc::O_ACCMODE | libc::O_TRUNC | libc::O_APPEND | libc::O_CREAT;
    if (!writable || directory) && value.0 & mutations != 0 {
        return Err(Errno::EROFS);
    }
    let allowed = libc::O_CLOEXEC
        | KERNEL_O_LARGEFILE
        | libc::O_NOFOLLOW
        | libc::O_DIRECTORY
        | libc::O_NOCTTY
        | libc::O_NONBLOCK
        | libc::O_NOATIME
        | if directory { 0 } else { KERNEL_FMODE_EXEC }
        | if writable && !directory {
            libc::O_ACCMODE | libc::O_APPEND
        } else {
            0
        };
    if value.0 & !allowed != 0 {
        return Err(Errno::EOPNOTSUPP);
    }
    let access = match value.0 & libc::O_ACCMODE {
        libc::O_RDONLY => FileAccess::ReadOnly,
        libc::O_WRONLY => FileAccess::WriteOnly,
        libc::O_RDWR => FileAccess::ReadWrite,
        _ => return Err(Errno::EINVAL),
    };
    Ok(FileOpenOptions {
        access,
        append: value.0 & libc::O_APPEND != 0,
        truncate: false,
    })
}
