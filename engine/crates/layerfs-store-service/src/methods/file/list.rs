//! `fs.list`.
use crate::methods::args::{optional_path, paging, snapshot, Answer};
use crate::{read::reads, StoreService};
use layerfs_bridge::control::SdkValue;

/// `list(path?, { branch, commit?, pageSize?, pagePos? })`: one window of a directory.
pub fn list(service: &StoreService, args: &SdkValue) -> Answer {
    let (reader, snap) = snapshot(service, args)?;
    let path = optional_path(args)?;
    reads::list(&reader, &snap, &path, paging(args))
}
