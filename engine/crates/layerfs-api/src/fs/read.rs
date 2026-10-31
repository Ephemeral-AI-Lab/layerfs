//! `fs.read`.
use crate::base::args::{invalid, line, path, snapshot, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::{read::reads, StoreService};

/// `read(path, { branch, commit?, startLine?, endLine?, binary?, offset? })`: one window of a file at a Branch, Commit or Layer.
pub fn read(service: &StoreService, args: &SdkValue) -> Answer {
    let (reader, snap) = snapshot(service, args)?;
    let path = path(args, "path")?;
    let binary = match args.get("binary") {
        None | Some(SdkValue::Null) => false,
        Some(SdkValue::Bool(binary)) => *binary,
        Some(_) => return Err(invalid("binary must be a boolean")),
    };
    let offset = match args.get("offset") {
        None | Some(SdkValue::Null) => Ok(None),
        Some(SdkValue::Text(offset)) => Ok(Some(offset.clone())),
        Some(_) => Err(invalid("offset is not a valid read cursor")),
    };
    reads::read(
        &reader,
        &snap,
        &path,
        line(args, "startLine", 1),
        line(args, "endLine", 9_007_199_254_740_991),
        binary,
        offset,
    )
}
