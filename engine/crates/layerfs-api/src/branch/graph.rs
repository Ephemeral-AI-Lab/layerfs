//! `branch.graph`.
use crate::base::args::{paging, project, text, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::{history::graph, StoreService};

/// `graph({ branch, pageSize?, pagePos? })`: own Commits, fork and publication.
pub fn graph(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let name = text(args, "branch")?;
    let (limit, pos) = paging(args)?;
    graph::graph(&service.store, &stack, name, limit, pos)
}
