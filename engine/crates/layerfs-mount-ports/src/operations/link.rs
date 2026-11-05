//! LINK: one more name for an existing file or symlink inode.
use layerfs_content::filesystem::PathName;
use layerfs_workspace::Operation;

/// Aliases share one serial, so the kernel keeps one inode, one attribute set
/// and one page cache for every name. A symlink keeps its opaque target;
/// LINK does not follow it. Workspace refuses directory aliases before effects.
pub fn link(serial: u64, parent: u64, name: PathName) -> Operation {
    Operation::Link {
        serial,
        parent,
        name,
    }
}
