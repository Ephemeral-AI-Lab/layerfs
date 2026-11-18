//! `fs.read`.
use crate::methods::args::{line, path, snapshot_full, Answer};
use crate::{
    read::{paged_reads, reads, snapshot, visibility},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `read(path, { branch, commit?, startLine?, endLine?, binary? })`: one window of a file at a
/// Branch, Commit or Layer; with `binary`, a file that is not text answers its size.
pub fn read(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, snap) = snapshot_full(service, args)?;
    let path = path(args, "path")?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap)?;
    if snapshot::entry(&reader, &snap.tree, &path)?.is_some() {
        if let Some(refusal) = visibility::refuse(&path, &seen) {
            return Err(refusal);
        }
    }
    let answer = reads::read(
        &reader,
        &snap,
        &path,
        line(args, "startLine", 1),
        line(args, "endLine", 9_007_199_254_740_991),
        crate::methods::args::flag(args, "binary", false)?,
    );
    paged_reads::pin(&service.store, &snap, answer)
}
