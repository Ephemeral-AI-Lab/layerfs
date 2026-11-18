//! `project.stats` and `project.hide_path` through the ordinary Store-call
//! route, each answering its call's full map (the keys of its long form;
//! the dispatcher shapes it).
use super::op::{next_fields, on_project, StoreOp};
use layerfs_bridge::control::SdkValue;
use layerfs_home::{events::Events, outcome::Outcome};
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

/// `project.hide_path(project, path)`.
#[derive(Clone, Debug, Default)]
pub struct ProjectHidePath {
    /// `<store>/<project>`.
    pub project: String,
    /// The absolute POSIX path.
    pub path: String,
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

/// `project.hide_path`: one path hidden from every node at or below its
/// marker frontier; it writes a marker, not a Commit, and frees no bytes.
/// A path already hidden answers `unchanged`.
pub fn hide_path(home: &Path, events: &mut Events<'_>, request: &ProjectHidePath) -> Outcome {
    let value = match on_project(home, events, &request.project, StoreOp::RemoveFile(request)) {
        Ok(value) => value,
        Err(refusal) => {
            // The detail names the project inside its Store, as the Store
            // does.
            let project = request
                .project
                .rsplit_once('/')
                .map_or(request.project.as_str(), |(_, project)| project);
            return Err(crate::unknown::named_unknown(
                refusal, "project", project, None,
            ));
        }
    };
    let unchanged = value.get_text("kind") == Some("UpToDate");
    let path = value.get_text("path").unwrap_or("");
    let path = match path.starts_with('/') {
        true => path.to_owned(),
        false => format!("/{path}"),
    };
    let hidden_at = (value.get_int("removed_ms")).map(layerfs_home::time::iso);
    Ok(SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("unchanged", SdkValue::Bool(unchanged)),
        (
            "outcome",
            SdkValue::text(if unchanged { "unchanged" } else { "hidden" }),
        ),
        ("path", SdkValue::Text(path)),
        (
            "frontier_serial",
            value.get("frontier_serial").cloned().unwrap_or_default(),
        ),
        ("hidden_at", SdkValue::opt_text(hidden_at)),
    ]))
}
