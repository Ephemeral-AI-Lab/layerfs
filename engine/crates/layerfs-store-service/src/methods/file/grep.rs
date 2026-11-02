//! `fs.grep`.
use crate::methods::args::{invalid, optional_path, paging, snapshot, text, Answer};
use crate::{read::reads, StoreService};
use layerfs_bridge::control::SdkValue;

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
