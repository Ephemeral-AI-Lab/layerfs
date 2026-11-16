//! `fs.stat`.
use crate::methods::args::{path, snapshot_full, Answer};
use crate::{
    read::{paged_reads, reads, snapshot, visibility},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `stat(path, { branch, commit? })`: type, inode, size and mode of one path.
pub fn stat(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, snap) = snapshot_full(service, args)?;
    let path = path(args, "path")?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap)?;
    if snapshot::entry(&reader, &snap.tree, &path)?.is_some() {
        if let Some(refusal) = visibility::refuse(&path, &seen) {
            return Err(refusal);
        }
    }
    paged_reads::pin(&service.store, &snap, reads::stat(&reader, &snap, &path))
}
