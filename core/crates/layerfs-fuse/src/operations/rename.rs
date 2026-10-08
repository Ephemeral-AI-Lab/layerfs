//! RENAME, including replacement: both names in one atomic owner job.
use super::{mutation::Declined, unsupported};
use layerfs_content::filesystem::PathName;
use layerfs_workspace::Operation;

/// `flags` are the kernel's rename flags. No destination path is supplied:
/// when a directory changes parent, the publishing job proves it is not moved
/// beneath itself from this connection's retained parent index.
pub fn rename(
    parent: u64,
    name: PathName,
    new_parent: u64,
    new_name: PathName,
    flags: u32,
) -> Result<Operation, Declined> {
    Ok(Operation::Rename {
        parent,
        name,
        new_parent,
        new_name,
        replace: unsupported::rename_replaces(flags)?,
        destination_path: None,
    })
}
