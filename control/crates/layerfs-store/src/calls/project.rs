//! Saved-tree project statistics through the ordinary Store-call route.
use super::op::{on_project, StoreOp};
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

/// Count one immutable saved tree, excluding the root directory.
pub fn stats(home: &Path, events: &mut Events<'_>, request: &ProjectStats) -> Outcome {
    on_project(
        home,
        events,
        &request.project,
        StoreOp::ProjectStats(request),
    )
}
