//! `fs.blame`.
use crate::history::rows::Rows;
use crate::methods::args::{line, path, snapshot_in, snapshot_target, Answer};
use crate::read::cached::Cached;
use crate::{
    read::{blame, visibility},
    StoreService,
};
use layerfs_protocol::provision::install::SdkValue;

/// `blame(path, { branch, commit?, fromLine? })`: each line with the Commit that last changed it.
pub fn blame(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, name) = snapshot_target(service, args)?;
    // The call's caches, from the root's open on, across every version
    // (#818 H3).
    let cached = Cached::new(&reader)?;
    let (snap, saved) = snapshot_in(service, &stack, &cached, name)?;
    let path = path(args, "path")?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap, saved.removals)?;
    // The call's rows, from the resolve's on (#818 H1).
    let mut rows = Rows::new(&service.store);
    rows.hold(snap.node, saved.record, saved.recorded);
    blame::blame(
        &mut rows,
        &cached,
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
        crate::read::versions::Batching::Provider,
    )
}
