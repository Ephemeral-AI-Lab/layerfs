//! `file.history`: a path can be absent at the pinned tip.
use crate::families::args::{path, project, text, Answer};
use crate::filesystem::{
    paged::{query, revision},
    root,
};
use crate::history::saved;
use crate::StoreService;
use layerfs_protocol::sdk::SdkValue;

pub fn history(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let branch = text(args, "branch")?;
    // a Commit id prefix is resolved first
    let pinned = crate::families::args::pinned(service, &stack, args)?;
    let at = pinned.as_deref().or(args.get_text("commit"));
    let node = saved::resolve_context(&service.store, &stack, branch, at)?;
    let reader = root::reader(&service.store, &stack);
    revision::list(
        &service.store,
        &reader,
        node,
        revision::Request {
            path: &path(args, "path")?,
            traversal: args.get_text("traversal").unwrap_or("provenance"),
            page: query::result_page(args)?,
            branch,
            continuation: args.get_text("continuation"),
            stack: stack.id,
            batching: crate::filesystem::versions::Batching::Provider,
        },
    )
}
