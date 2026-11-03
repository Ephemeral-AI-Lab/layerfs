//! LINK: one more name for an existing regular file's single serial.
use layerfs_content::filesystem::PathName;
use layerfs_workspace::Operation;

/// Aliases share one serial, so the kernel keeps one inode, one attribute set
/// and one page cache for every name. A directory or symlink target is
/// refused by Workspace before any effect.
pub fn link(serial: u64, parent: u64, name: PathName) -> Operation {
    Operation::Link {
        serial,
        parent,
        name,
    }
}
