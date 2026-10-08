//! UNLINK and RMDIR: one removed binding. The inode's kernel lookup, open and
//! processing owners are independent of its names and are not touched here.
use layerfs_content::filesystem::PathName;
use layerfs_workspace::Operation;

pub fn unlink(parent: u64, name: PathName) -> Operation {
    Operation::Unlink { parent, name }
}
pub fn rmdir(parent: u64, name: PathName) -> Operation {
    Operation::Rmdir { parent, name }
}
