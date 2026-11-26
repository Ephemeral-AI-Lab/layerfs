//! `fs.stat`.
use crate::methods::args::{path, snapshot_full, Answer};
use crate::read::cached::Cached;
use crate::{
    read::{paged_reads, reads, visibility},
    StoreService,
};
use layerfs_bridge::control::SdkValue;
use layerfs_content::compare::memo::Memo;

/// `stat(path, { branch, commit? })`: type, inode, size and mode of one path.
pub fn stat(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, snap, saved) = snapshot_full(service, args)?;
    let path = path(args, "path")?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap, saved.removals)?;
    // One path walk for the call (#818 R3), through its caches and memo.
    let cached = Cached::new(&reader)?;
    let memo = Memo::new(&cached);
    let entry = visibility::checked(&memo, &snap, &path, &seen)?;
    let answer = reads::stat(&reader, &cached, &snap, &path, entry);
    paged_reads::pin(&service.store, &snap, &saved.record, answer)
}
