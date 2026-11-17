//! `project.list_branches`, `project.stats` and `project.remove_file`.
use crate::methods::args::{invalid, paging, project, text, Answer};
use crate::{
    history::{branch, lookup},
    store::Store,
    StoreService,
};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::compare::TreeTable;
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_history::{
    LayerStackRecord, NodeId, PathRemovalKind, PathRemovalRequest, MAXIMUM_PAGE_RECORDS,
};
use std::sync::Arc;

/// `list_branches({ name?, pageSize?, pagePos? })`: every Branch with its id, base and head, and main, in name order.
pub fn list_branches(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let (limit, pos) = if args.get("page").is_some() {
        let page = crate::read::query::result_page(args)?;
        (page.limit, page.skip)
    } else {
        paging(args)?
    };
    let name = match args.get("name") {
        None => None,
        Some(SdkValue::Text(name)) => Some(name.as_str()),
        Some(_) => return Err(invalid("name must be text")),
    };
    let mut result = branch::list_bounded(
        &service.store,
        &stack,
        limit,
        pos,
        name,
        args.get("page")
            .map(|_| crate::read::query::MAXIMUM_QUERY_WORK as usize),
    )?;
    if args.get("page").is_some() && result.get_bool("hasMore") {
        if let Some(SdkValue::List(items)) = result.get("items") {
            if items.len() < limit {
                return Err(crate::read::query::capacity(
                    "branch inventory page exceeds reply budget; reduce page_size",
                ));
            }
        }
    }
    if let SdkValue::Map(fields) = &mut result {
        if let Some((_, SdkValue::List(items))) = fields.iter_mut().find(|(k, _)| k == "items") {
            for item in items {
                let id = item
                    .get_text("head")
                    .or_else(|| item.get_text("base"))
                    .ok_or_else(|| invalid("branch has no saved head"))?;
                let node = crate::history::lookup::node(id)?;
                let reference = crate::history::saved::reference(&service.store, node)?;
                if let SdkValue::Map(fields) = item {
                    fields.push(("savedHead".into(), reference));
                }
            }
        }
        fields.push(("consistency".into(), SdkValue::text("live")));
    }
    Ok(result)
}

/// `project.stats`: resolve the Branch-qualified immutable pin once, then count it.
pub fn stats(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let branch = args.get_text("branch").unwrap_or("main");
    let node = crate::history::saved::resolve_context(
        &service.store,
        &stack,
        branch,
        args.get_text("commit"),
    )?;
    let reader = crate::read::snapshot::reader(&service.store, &stack);
    let snap = crate::read::snapshot::resolve(
        &service.store,
        &stack,
        &reader,
        &crate::history::lookup::node_text(node),
    )?;
    crate::read::tree_stats::count(
        &service.store,
        &reader,
        &snap,
        crate::methods::args::text(args, "project")?,
    )
}

/// The roots of the trees a read of the project can see: main's head Layer
/// and every Branch head, deduplicated by root.
fn visible_trees(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
) -> Result<Vec<layerfs_content::ObjectId>, SdkRefusal> {
    let mut roots = vec![lookup::root(store, NodeId::Layer(stack.head_layer))?];
    let mut page = layerfs_history::Page {
        cursor: None,
        limit: MAXIMUM_PAGE_RECORDS,
    };
    loop {
        let found = store
            .published(|history| history.branches(stack.id, &page))
            .map_err(lookup::published)?;
        for branch in found.records {
            let root = lookup::root(store, lookup::branch_node(&branch))?;
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
        match found.continuation {
            Some(cursor) => page.cursor = Some(cursor),
            None => break,
        }
    }
    Ok(roots)
}

/// `project.remove_file({ project, path })`: hide one path from every node at
/// or below the frontier. Visibility only; no byte is freed in 2.0.
pub fn remove_file(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let requested = text(args, "path")?;
    let normalized = layerfs_history::path::NormalizedPath::new(requested)
        .map_err(|why| invalid(format!("path: {why}")))?;
    // The marker is project-wide, so the path need not exist on main: it may
    // exist only on a Branch, or never have existed. Spec 4.3 authorizes one
    // refusal shape, a name below an intermediate symlink, which is ill-typed
    // rather than absent; check it against every visible tree. A regular-file
    // intermediate is not refused: the child is merely absent.
    let reader = crate::read::snapshot::reader(&service.store, &stack);
    let absolute = format!("/{}", normalized.as_str());
    for root in visible_trees(&service.store, &stack)? {
        let tree = TreeTable::open(&*reader, FilesystemRootId(root)).map_err(lookup::content)?;
        let _ = crate::read::snapshot::entry(&reader, &tree, &absolute)?;
    }
    let removed_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        });
    let intent = crate::history::admission::project_removal(&stack);
    let outcome = service
        .store
        .history()
        .gc_remove_path(
            &PathRemovalRequest {
                stack: stack.id,
                path: normalized.as_bytes().to_vec(),
                removed_ms,
            },
            &intent,
        )
        .map_err(lookup::history)?;
    Ok(SdkValue::map([
        (
            "kind",
            SdkValue::text(match outcome.kind {
                PathRemovalKind::Removed => "Removed",
                PathRemovalKind::UpToDate => "UpToDate",
            }),
        ),
        ("project", SdkValue::text(text(args, "project")?)),
        (
            "path",
            SdkValue::text(String::from_utf8_lossy(&outcome.path).as_ref()),
        ),
        (
            "frontier_serial",
            SdkValue::Int(outcome.frontier_serial as i64),
        ),
        ("removed_ms", SdkValue::Int(outcome.removed_ms)),
    ]))
}
