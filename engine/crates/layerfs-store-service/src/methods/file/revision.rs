//! `fs.revision`: a path can be absent at the pinned tip.
use crate::history::saved;
use crate::methods::args::{path, project, text, Answer};
use crate::read::{query, revision, snapshot};
use crate::StoreService;
use layerfs_bridge::control::SdkValue;

pub fn revision(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let branch = text(args, "branch")?;
    let node = saved::resolve_context(&service.store, &stack, branch, args.get_text("commit"))?;
    let reader = snapshot::reader(&service.store, &stack);
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
        },
    )
}
