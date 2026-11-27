//! `project.hide_path` through the ordinary Store-call route, answering its
//! call's full map (the keys of its long form; the dispatcher shapes it).
use crate::admin::route::methods::{on_project, StoreOp};
use layerfs_home::reply::{events::Events, outcome::Outcome};
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

/// `project.hide_path(project, path)`.
#[derive(Clone, Debug, Default)]
pub struct ProjectHidePath {
    /// `<store>/<project>`.
    pub project: String,
    /// The absolute POSIX path.
    pub path: String,
}

/// `project.hide_path`: one path hidden from every node at or below its
/// marker frontier; it writes a marker, not a Commit, and frees no bytes.
/// A path already hidden answers `unchanged`.
pub fn hide_path(home: &Path, events: &mut Events<'_>, request: &ProjectHidePath) -> Outcome {
    let value = match on_project(home, events, &request.project, StoreOp::HidePath(request)) {
        Ok(value) => value,
        Err(refusal) => {
            // The detail names the project inside its Store, as the Store
            // does.
            let project = request
                .project
                .rsplit_once('/')
                .map_or(request.project.as_str(), |(_, project)| project);
            return Err(crate::data::unknown::named_unknown(
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
    let hidden_at = (value.get_int("removed_ms")).map(layerfs_home::reply::time::iso);
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
