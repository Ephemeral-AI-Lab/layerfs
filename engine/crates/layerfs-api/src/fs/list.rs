//! `fs.list`.
use crate::base::args::{optional_path, paging, snapshot, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::{read::reads, StoreService};

/// `list(path?, { branch, commit?, pageSize?, pagePos? })`: one window of a directory.
pub fn list(service: &StoreService, args: &SdkValue) -> Answer {
    let (reader, snap) = snapshot(service, args)?;
    let path = optional_path(args)?;
    reads::list(&reader, &snap, &path, paging(args))
}
