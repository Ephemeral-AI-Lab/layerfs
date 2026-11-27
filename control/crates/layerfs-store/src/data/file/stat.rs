//! `file.stat`: one path's kind, size, mode and Commit.
use super::read::{date, octal, pin, run, take};
use super::requests::FileStat;
use crate::admin::route::methods::Args;
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

/// `file.stat`: one path's type, size, mode and modification time.
pub fn stat(home: &Path, events: &mut Events<'_>, request: &FileStat) -> Outcome {
    let mut args = Args::default();
    args.at(&request.branch, request.at.as_deref())
        .text("path", &request.path);
    let value = run(home, events, &request.project, "fs.stat", args)?;
    Ok(SdkValue::map([
        ("type", take(&value, "type")),
        ("size", take(&value, "size")),
        ("mode", octal(&take(&value, "mode"))),
        ("commit", pin(&value)),
        ("root", take(&value, "root")),
        ("path", take(&value, "path")),
        ("inode", take(&value, "inode")),
        ("modified_at", date(value.get("modifiedMs"))),
    ]))
}
