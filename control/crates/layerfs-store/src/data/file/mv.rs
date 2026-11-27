//! `file.mv`: a path moved, as one Commit.
use super::requests::FileMv;
use super::write::{base, commit};
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::Outcome;
use std::path::Path;

/// `file.mv`: a path moved, as one Commit.
pub fn mv(home: &Path, events: &mut Events<'_>, request: &FileMv) -> Outcome {
    let mut args = base(&request.branch, &request.guard);
    args.text("from", &request.from).text("to", &request.to);
    commit(home, events, &request.project, "fs.mv", args)
}
