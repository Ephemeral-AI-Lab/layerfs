//! `project.stats` and `project.remove_file` (the SDK's `project.hide_path`).
use crate::families::args::{invalid, project, text, Answer};
use crate::{history::resolve, lookup, store::Store, StoreService};
use layerfs_content::compare::TreeTable;
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_history::{
    LayerStackRecord, NodeId, PathRemovalKind, PathRemovalRequest, MAXIMUM_PAGE_RECORDS,
};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::sync::Arc;

/// `project.stats`: resolve the Branch-qualified immutable pin once, then
/// count it. The pin (`commit`) is a full id or a prefix of 8 or more
/// characters, resolved by the catalog in one read; it must be in the
/// Branch's history.
pub fn stats(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let branch = args.get_text("branch").unwrap_or("main");
    let pin = match args.get_text("commit") {
        Some(text) => Some(resolve::commit(&service.store, &stack, text)?.text()),
        None => None,
    };
    let node =
        crate::history::saved::resolve_context(&service.store, &stack, branch, pin.as_deref())?;
    let reader = crate::filesystem::root::reader(&service.store, &stack);
    let snap = crate::filesystem::root::resolve(
        &service.store,
        &stack,
        &reader,
        &layerfs_protocol::sdk::node_text(node),
    )?;
    crate::filesystem::tree::stats::count(
        &service.store,
        &reader,
        &snap,
        crate::families::args::text(args, "project")?,
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
    let normalized = layerfs_history::catalog::path::NormalizedPath::new(requested)
        .map_err(|why| invalid(format!("path: {why}")))?;
    // The marker is project-wide, so the path need not exist on main: it may
    // exist only on a Branch, or never have existed. Spec 4.3 authorizes one
    // refusal shape, a name below an intermediate symlink, which is ill-typed
    // rather than absent; check it against every visible tree. A regular-file
    // intermediate is not refused: the child is merely absent.
    let reader = crate::filesystem::root::reader(&service.store, &stack);
    let absolute = format!("/{}", normalized.as_str());
    for root in visible_trees(&service.store, &stack)? {
        let tree = TreeTable::open(&*reader, FilesystemRootId(root)).map_err(lookup::content)?;
        let _ = crate::filesystem::root::entry(&reader, &tree, &absolute)?;
    }
    let removed_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        });
    let intent = crate::history::gc_admission::project_removal(&stack);
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
        .map_err(|error| {
            // A remove_file writes its marker in one transaction and takes
            // no durable op of its own, so phase and op are null.
            lookup::history_in(error, intent.scope_kind, stack.name.as_str(), None, None)
        })?;
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
