//! CREATE, MKNOD (regular), MKDIR and SYMLINK: one new inode and its entry.
use super::{mutation::Declined, unsupported};
use layerfs_content::filesystem::{PathName, SymlinkTarget};
use layerfs_workspace::{Operation, Refusal};

/// The kernel has already applied the caller's umask. Bits outside the
/// portable grammar (set-id, set-gid) reach Workspace and are refused there.
const MODE_BITS: u32 = 0o7777;

pub fn create(parent: u64, name: PathName, mode: u32) -> Operation {
    Operation::Create {
        parent,
        name,
        mode: mode & MODE_BITS,
    }
}
/// MKNOD of a regular file is an ordinary create without a descriptor.
pub fn node(parent: u64, name: PathName, mode: u32) -> Result<Operation, Declined> {
    unsupported::node(mode)?;
    Ok(create(parent, name, mode))
}
pub fn mkdir(parent: u64, name: PathName, mode: u32) -> Operation {
    Operation::Mkdir {
        parent,
        name,
        mode: mode & MODE_BITS,
    }
}
/// The kernel reads a link target back into one page with a terminator, so a
/// target longer than one page minus one could never be returned whole. It is
/// refused here, at creation, for the page size this connection recorded.
pub fn symlink(
    parent: u64,
    name: PathName,
    target: &[u8],
    page_size: u32,
) -> Result<Operation, Declined> {
    if target.len() >= page_size as usize {
        return Err(Declined::TargetTooLong);
    }
    let target =
        SymlinkTarget::new(target.to_vec()).map_err(|_| Declined::Refused(Refusal::Invalid))?;
    Ok(Operation::Symlink {
        parent,
        name,
        target,
    })
}
