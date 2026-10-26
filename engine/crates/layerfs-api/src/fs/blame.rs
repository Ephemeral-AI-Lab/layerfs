//! `fs.blame`.
use crate::base::args::{line, path, snapshot, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::{read::blame, StoreService};

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
