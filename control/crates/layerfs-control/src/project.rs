//! The `project` call that checks who uses a project, `project.remove`
//! (layerfs-project#105); the other `project` calls are `layerfs-store`'s
//! own.
use layerfs_home::events::Events;
use layerfs_home::names::ProjectRef;
use layerfs_home::outcome::Outcome;
use std::path::Path;

/// `project.remove`: a project's history dropped from its Store, once its
/// Sandboxes are closed.
pub fn remove(home: &Path, events: &mut Events<'_>, project: &ProjectRef) -> Outcome {
    layerfs_store::project::family::remove(home, events, project, crate::store::RELATED)
}
