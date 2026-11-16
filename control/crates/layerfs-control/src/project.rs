//! The `project` call that retires a project in its Store and removes the
//! caller's home entry (`project.delete`, spec §4); the other `project`
//! calls are `layerfs-store`'s own.
use crate::inventory;
use layerfs_home::events::Events;
use layerfs_home::names::ProjectRef;
use layerfs_home::outcome::Outcome;
use std::path::Path;

/// The project-scope `InUse` checks `project.delete` takes from this layer.
pub const PROJECT_RELATED: layerfs_store::Related = layerfs_store::Related {
    store_idle: inventory::store_idle,
    can_remove_project: inventory::can_remove_project,
};

/// `project.delete(project)`: retires the project in its Store and removes
/// the caller home entry (spec §4).
pub fn delete(home: &Path, events: &mut Events<'_>, project: &ProjectRef) -> Outcome {
    layerfs_store::project::delete::delete(home, events, project, PROJECT_RELATED)
}
