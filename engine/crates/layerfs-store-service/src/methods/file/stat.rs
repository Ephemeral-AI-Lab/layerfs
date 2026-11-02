//! `fs.stat`.
use crate::methods::args::{path, snapshot, Answer};
use crate::{read::reads, StoreService};
use layerfs_bridge::control::SdkValue;

/// `stat(path, { branch, commit? })`: type, inode, size and mode of one path.
pub fn stat(service: &StoreService, args: &SdkValue) -> Answer {
    let (reader, snap) = snapshot(service, args)?;
    reads::stat(&reader, &snap, &path(args, "path")?)
}
