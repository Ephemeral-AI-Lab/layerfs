//! `file.cp`: a path copied, as one Commit.
use super::requests::FileMv;
use super::write::{base, commit};
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::Outcome;
use std::path::Path;

/// `file.cp`: a path copied, a folder with everything in it, as one Commit.
pub fn cp(home: &Path, events: &mut Events<'_>, request: &FileMv) -> Outcome {
    let mut args = base(&request.branch, &request.guard);
    args.text("from", &request.from).text("to", &request.to);
    commit(home, events, &request.project, "fs.cp", args)
}
