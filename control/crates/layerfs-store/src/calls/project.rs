//! Saved-tree project statistics through the ordinary Store-call route.
use super::op::{on_project, StoreOp};
use layerfs_bridge::control::SdkValue;
use layerfs_home::{events::Events, outcome::Outcome};
use std::path::Path;

/// One pinned project's entry-count request.
#[derive(Clone, Debug)]
pub struct ProjectStats {
    /// `<store>/<project>`.
    pub project: String,
    /// Branch context, `main` by default.
    pub branch: String,
    /// Optional full Commit or Layer id admitted by that context.
    pub commit: Option<String>,
}

/// `project.remove_file(project, path)`.
#[derive(Clone, Debug, Default)]
pub struct ProjectRemoveFile {
    /// `<store>/<project>`.
    pub project: String,
    /// The absolute POSIX path.
    pub path: String,
}

/// Count one immutable saved tree, excluding the root directory.
pub fn stats(home: &Path, events: &mut Events<'_>, request: &ProjectStats) -> Outcome {
    on_project(
        home,
        events,
        &request.project,
        StoreOp::ProjectStats(request),
    )
}

/// `project.remove_file`: one path hidden from every node at or below
/// its marker frontier. The result names the project as the caller gave it,
/// `<store>/<project>`.
pub fn remove_file(home: &Path, events: &mut Events<'_>, request: &ProjectRemoveFile) -> Outcome {
    let value = match on_project(home, events, &request.project, StoreOp::RemoveFile(request)) {
        Ok(value) => value,
        Err(refusal) => {
            // The detail names the project inside its Store, as the Store
            // does; the result below keeps the caller's `<store>/<project>`
            // spelling.
            let project = request
                .project
                .rsplit_once('/')
                .map_or(request.project.as_str(), |(_, project)| project);
            return Err(crate::unknown::named_unknown(
                refusal, "project", project, None,
            ));
        }
    };
    Ok(layerfs_home::home::with_field(
        value,
        "project",
        SdkValue::text(&request.project),
    ))
}
