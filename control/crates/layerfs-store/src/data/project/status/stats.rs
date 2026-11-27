//! `project.stats` through the ordinary Store-call route, answering its
//! call's full map (the keys of its long form; the dispatcher shapes it).
use crate::admin::route::methods::{next_fields, on_project, StoreOp};
use layerfs_home::reply::{events::Events, outcome::Outcome};
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

/// `project.stats(project, { branch, at })`.
#[derive(Clone, Debug)]
pub struct ProjectStats {
    /// `<store>/<project>`.
    pub project: String,
    /// The Branch counted, `main` by default.
    pub branch: String,
    /// The Commit of the Branch's history counted instead of its head: a
    /// full id, or a prefix of 8 or more characters.
    pub at: Option<String>,
}

/// Counts one immutable saved tree, the root directory left out:
/// `file_count`, `dir_count`, `symlink_count`, `size` (the length the
/// counted node records), `partial` when the count stopped at its work
/// bound, and the counted `commit` and its `root`. A prefix that matches
/// more than one Commit is refused `InvalidInput` with `matches`.
pub fn stats(home: &Path, events: &mut Events<'_>, request: &ProjectStats) -> Outcome {
    let counted = on_project(
        home,
        events,
        &request.project,
        StoreOp::ProjectStats(request),
    )
    .map_err(next_fields)?;
    let at = counted.get("at");
    let copied = |name: &str| counted.get(name).cloned().unwrap_or_default();
    let of_at = |name: &str| at.and_then(|at| at.get(name)).cloned().unwrap_or_default();
    Ok(SdkValue::map([
        ("file_count", copied("files")),
        ("dir_count", copied("directories")),
        ("symlink_count", copied("symlinks")),
        ("size", copied("logical")),
        (
            "partial",
            SdkValue::Bool(counted.get("complete") == Some(&SdkValue::Bool(false))),
        ),
        ("commit", of_at("id")),
        ("root", of_at("root")),
    ]))
}
