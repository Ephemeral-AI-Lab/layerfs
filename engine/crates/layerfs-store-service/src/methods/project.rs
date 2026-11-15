//! `project.list_branches`, `project.stats` and `project.remove_file`.
use crate::methods::args::{invalid, paging, project, text, Answer};
use crate::{
    history::{branch, lookup},
    StoreService,
};
use layerfs_bridge::control::SdkValue;
use layerfs_history::{PathRemovalKind, PathRemovalRequest};

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

/// `project.remove_file({ project, path })`: hide one path from every node at
/// or below the frontier. Visibility only; no byte is freed in 2.0.
pub fn remove_file(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let requested = text(args, "path")?;
    let normalized = layerfs_history::path::NormalizedPath::new(requested)
        .map_err(|why| invalid(format!("path: {why}")))?;
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
