//! A Store's own project listing (T4.5c), read with the same
//! `layerfs_project::store::projects` wherever the Store's calls run: in
//! this process for a sqlite Store on this machine and for a PostgreSQL
//! Store, in the machine's worker (`project.list` worker op) for a sqlite
//! Store on another machine, so a project retired in the Store is hidden on
//! every route. Each project comes with main's head Layer, which its stack
//! record holds, and, when asked for, the length that Layer records.
use crate::open_args::OpenArgs;
use crate::project::frame::{ListedProject, ListingFrame, ListingReply};
use crate::service;
use layerfs_protocol::provision::install::SdkRefusal;
use layerfs_protocol::sdk::node_text;
use layerfs_history::NodeId;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store_service::history::lookup;
use layerfs_store_service::StoreService;
use std::path::Path;

/// The live projects of the open Store `service`, in name order: each with
/// main's head (its stack record holds it) and, when `stats`, the length
/// that head Layer records (one Layer record per project, no tree walk).
pub fn listed(
    service: &StoreService,
    stats: bool,
) -> std::result::Result<Vec<ListedProject>, SdkRefusal> {
    service
        .store
        .published(|history| {
            let stacks = layerfs_project::store::projects(history)?;
            let mut out = Vec::with_capacity(stacks.len());
            for stack in stacks {
                let size = match stats {
                    true => history
                        .layer(stack.head_layer)?
                        .map(|layer| i64::try_from(layer.logical).unwrap_or(i64::MAX)),
                    false => None,
                };
                out.push(ListedProject {
                    name: stack.name.as_str().to_owned(),
                    stack: stack.id,
                    head: Some(node_text(NodeId::Layer(stack.head_layer))),
                    size,
                });
            }
            Ok(out)
        })
        .map_err(lookup::published)
}

/// The `project.list` worker op: Store `store`'s live projects, in name
/// order, under `root` (the worker's own root). The caller intersects the
/// answer with its own home entries (`project.list`, `project.get`).
pub fn list_projects(root: &Path, frame: &ListingFrame) -> Outcome {
    let store = frame.store.as_str();
    let store_record = frame
        .store_record
        .clone()
        .ok_or_else(|| refused("InvalidInput", "project.list needs the Store's record"))?;
    let open = OpenArgs::in_root(root, store, Some(store_record));
    let projects = service::with_service(&open, |service| listed(service, frame.stats))?;
    Ok(ListingReply { projects }.encode())
}
