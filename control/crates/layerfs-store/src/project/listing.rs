//! `project.list` in a worker (T4.5c): the Store's filtered project
//! listing, read under the worker's own root with the same
//! `layerfs_project::store::projects` a local or PostgreSQL Store uses, so
//! a project retired in the Store is hidden on that route too.
use crate::open_args::OpenArgs;
use crate::project::frame::{ListedProject, ListingFrame, ListingReply};
use crate::service;
use layerfs_home::home::paths;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store_service::history::lookup;
use std::path::Path;

/// The `project.list` worker op: Store `store`'s live projects, in name
/// order, under `root` (the worker's own root). The caller intersects the
/// answer with its own home entries (`project.list`, `project.get`).
pub fn list_projects(root: &Path, frame: &ListingFrame) -> Outcome {
    let store = frame.store.as_str();
    let store_record = frame
        .store_record
        .clone()
        .ok_or_else(|| refused("InvalidInput", "project.list needs the Store's record"))?;
    let open = OpenArgs {
        home: Some(root.to_path_buf()),
        store: Some(store.to_owned()),
        store_path: Some(paths::store_file(root, store).display().to_string()),
        store_record: Some(store_record),
        postgresql: None,
        ..OpenArgs::default()
    };
    let stacks = service::with_service(&open, |service| {
        service
            .store
            .published(layerfs_project::store::projects)
            .map_err(lookup::published)
    })?;
    Ok(ListingReply {
        projects: stacks
            .into_iter()
            .map(|stack| ListedProject {
                name: stack.name.as_str().to_owned(),
                stack: stack.id,
            })
            .collect(),
    }
    .encode())
}
