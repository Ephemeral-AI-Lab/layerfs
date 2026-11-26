//! `fs.blame`.
use crate::methods::args::{line, path, snapshot_full, Answer};
use crate::{
    read::{blame, visibility},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `blame(path, { branch, commit?, fromLine? })`: each line with the Commit that last changed it.
pub fn blame(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, snap, saved) = snapshot_full(service, args)?;
    let path = path(args, "path")?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap, saved.removals)?;
    blame::blame(
        &service.store,
        &reader,
        &snap,
        &path,
        &seen,
        blame::Window {
            from: line(args, "fromLine", 1),
            count: line(args, "lineCount", 2000),
        },
        &mut blame::Limits {
            versions: crate::read::query::MAXIMUM_QUERY_WORK,
            bytes: 64 * 1024 * 1024,
        },
    )
}
