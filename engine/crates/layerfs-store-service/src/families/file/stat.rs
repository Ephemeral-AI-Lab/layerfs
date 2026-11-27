//! `fs.stat`.
use crate::families::args::{path, snapshot_in, snapshot_target, Answer};
use crate::filesystem::cache::Cached;
use crate::{
    filesystem::{
        paged::results,
        tree::{positional, visibility},
    },
    StoreService,
};
use layerfs_content::compare::memo::Memo;
use layerfs_protocol::sdk::SdkValue;

/// `stat(path, { branch, commit? })`: type, inode, size and mode of one path.
pub fn stat(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, name) = snapshot_target(service, args)?;
    // The call's caches, from the root's open on (#818 R4).
    let cached = Cached::new(&reader)?;
    let (snap, saved) = snapshot_in(service, &stack, &cached, name)?;
    let path = path(args, "path")?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap, saved.removals)?;
    // One path walk for the call (#818 R3), through its caches and memo.
    let memo = Memo::new(&cached);
    let entry = visibility::checked(&memo, &snap, &path, &seen)?;
    let answer = positional::stat(&cached, &snap, &path, entry);
    results::pin(
        &service.store,
        &snap,
        &saved.record,
        &saved.recorded,
        answer,
    )
}
