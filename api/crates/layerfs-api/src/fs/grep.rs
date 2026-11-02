//! `fs.grep`.
use crate::base::args::{invalid, optional_path, paging, snapshot, text, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::{read::reads, StoreService};

/// `grep(pattern, { branch, commit?, path?, pageSize?, pagePos? })`: literal matches per line in text files.
pub fn grep(service: &StoreService, args: &SdkValue) -> Answer {
    let (reader, snap) = snapshot(service, args)?;
    let pattern = text(args, "pattern")?;
    if pattern.is_empty() {
        return Err(invalid("pattern is empty"));
    }
    let path = optional_path(args)?;
    let (limit, pos) = paging(args)?;
    reads::grep(&reader, &snap, pattern, &path, limit, pos)
}
