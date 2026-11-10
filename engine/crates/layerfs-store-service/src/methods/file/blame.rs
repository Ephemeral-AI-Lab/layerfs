//! `fs.blame`.
use crate::methods::args::{line, path, snapshot, Answer};
use crate::{read::blame, StoreService};
use layerfs_bridge::control::SdkValue;

/// `blame(path, { branch, commit?, fromLine? })`: each line with the Commit that last changed it.
pub fn blame(service: &StoreService, args: &SdkValue) -> Answer {
    let (reader, snap) = snapshot(service, args)?;
    let path = path(args, "path")?;
    blame::blame(
        &service.store,
        &reader,
        &snap,
        &path,
        line(args, "fromLine", 1),
        line(args, "lineCount", 2000),
        &mut blame::Limits {
            versions: crate::read::query::MAXIMUM_QUERY_WORK,
            bytes: 64 * 1024 * 1024,
        },
    )
}
