//! `fs.blame`.
use crate::methods::args::{line, path, snapshot_in, snapshot_target, Answer};
use crate::read::cached::Cached;
use crate::{
    read::{blame, visibility},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `blame(path, { branch, commit?, fromLine? })`: each line with the Commit that last changed it.
pub fn blame(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, name) = snapshot_target(service, args)?;
    let (snap, saved) = {
        // The root opens through caches of its own until blame's reads share
        // one (#818 H3).
        let cached = Cached::new(&reader)?;
        snapshot_in(service, &stack, &cached, &name)?
    };
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
