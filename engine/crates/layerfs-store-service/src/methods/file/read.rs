//! `fs.read`.
use crate::methods::args::{line, path, snapshot, Answer};
use crate::{read::reads, StoreService};
use layerfs_bridge::control::SdkValue;

/// `read(path, { branch, commit?, startLine?, endLine? })`: one window of a file at a Branch, Commit or Layer.
pub fn read(service: &StoreService, args: &SdkValue) -> Answer {
    let (reader, snap) = snapshot(service, args)?;
    let path = path(args, "path")?;
    reads::read(
        &reader,
        &snap,
        &path,
        line(args, "startLine", 1),
        line(args, "endLine", 9_007_199_254_740_991),
    )
}
