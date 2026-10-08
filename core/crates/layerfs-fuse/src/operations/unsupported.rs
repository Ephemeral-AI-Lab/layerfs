//! The refusals that remain on purpose, decided before any engine custody.
use super::mutation::Declined;
use layerfs_workspace::Refusal;

const TYPE_MASK: u32 = 0o170000;
const REGULAR: u32 = 0o100000;
/// Linux rename flag values; any other bit is an unknown request.
const NO_REPLACE: u32 = 1;

/// How a request with no implementation is answered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Absent {
    /// `ENOSYS`: the kernel stops asking on this connection and uses its own
    /// fallback, such as local locks or a read/write copy.
    NotImplemented,
}
/// Extended attributes, ACCESS, READDIRPLUS, locks, BMAP, IOCTL, POLL, LSEEK,
/// FALLOCATE and COPY_FILE_RANGE. No capability for any of them is requested.
pub const fn absent() -> Absent {
    Absent::NotImplemented
}
/// The canonical format has regular files, directories and symlinks only: a
/// FIFO, socket or device node is not permitted. A zero type is a regular file.
pub fn node(mode: u32) -> Result<(), Declined> {
    match mode & TYPE_MASK {
        0 | REGULAR => Ok(()),
        _ => Err(Declined::Refused(Refusal::NotPermitted)),
    }
}
/// Ownership is the runtime's configured identity for every inode and is not
/// stored. Naming that same identity changes nothing; any other is refused.
pub fn ownership(
    identity: (u32, u32),
    owner: Option<u32>,
    group: Option<u32>,
) -> Result<(), Declined> {
    if owner.is_some_and(|owner| owner != identity.0)
        || group.is_some_and(|group| group != identity.1)
    {
        return Err(Declined::Refused(Refusal::NotPermitted));
    }
    Ok(())
}
/// Whether a rename may replace its destination. Exchange and whiteout
/// renames are not implemented and are invalid requests here.
pub fn rename_replaces(flags: u32) -> Result<bool, Declined> {
    match flags {
        0 => Ok(true),
        NO_REPLACE => Ok(false),
        _ => Err(Declined::Refused(Refusal::Invalid)),
    }
}
