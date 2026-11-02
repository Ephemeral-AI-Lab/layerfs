//! `fs.glob`.
use crate::methods::args::{optional_path, paging, snapshot, text, Answer};
use crate::{read::reads, StoreService};
use layerfs_bridge::control::SdkValue;

/// `glob(pattern, { branch, commit?, path?, pageSize?, pagePos? })`: paths matching a `/.lww`-syntax pattern.
pub fn glob(service: &StoreService, args: &SdkValue) -> Answer {
    let (reader, snap) = snapshot(service, args)?;
    let pattern = text(args, "pattern")?;
    let path = optional_path(args)?;
    let (limit, pos) = paging(args)?;
    reads::glob(&reader, &snap, pattern, &path, limit, pos)
}
