//! `file.mkdir`: a folder and its missing parents made, as one Commit.
use super::requests::FileMkdir;
use super::write::{base, commit, mode};
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::Outcome;
use std::path::Path;

/// `file.mkdir`: a folder and its missing parents made, as one Commit.
pub fn mkdir(home: &Path, events: &mut Events<'_>, request: &FileMkdir) -> Outcome {
    let mut args = base(&request.branch, &request.guard);
    args.text("path", &request.path)
        .int("mode", mode(request.mode.as_deref())?);
    commit(home, events, &request.project, "fs.mkdir", args)
}
