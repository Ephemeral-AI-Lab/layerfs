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
    )
}
