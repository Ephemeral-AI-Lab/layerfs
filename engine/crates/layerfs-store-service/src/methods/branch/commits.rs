//! Branch saved history at a pinned start, with matching-result pages.
use crate::{
    history::{lookup as history, pages, saved, MAIN},
    methods::args::{project, text, Answer},
    read::query::{result_page, MAXIMUM_QUERY_WORK},
    StoreService,
};
use layerfs_bridge::control::SdkValue;
use layerfs_history::NodeId;

/// Main follows publication parents; side branches follow persisted Commit parents.
pub fn commits(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let branch = text(args, "branch")?;
    let page = result_page(args)?;
    let start = saved::resolve_context(&service.store, &stack, branch, args.get_text("atCommit"))?;
    let key = pages::Key {
        store: std::sync::Arc::as_ptr(&service.store) as usize,
        start,
        branch: branch.to_owned(),
        size: page.limit,
        page: page.number,
    };
    let cached = pages::find(&service.store, &key);
    let mut current = cached.unwrap_or(Some(start));
    let skip = if cached.is_some() { 0 } else { page.skip };
    let mut next_page = None;
    let mut items = Vec::new();
    let mut seen = 0u64;
    let mut examined = 0u64;
    let mut bytes = 0usize;
    let mut more = false;
    while let Some(node) = current {
        if examined == MAXIMUM_QUERY_WORK {
            return Err(history::refuse("Capacity","saved history page exceeds query work bound; use a newer starting version or smaller page",SdkValue::Null));
        }
        examined += 1;
        current = match node {
            NodeId::Layer(id) if branch == MAIN => history::layer(&service.store, id)?
                .parent
                .map(NodeId::Layer),
            NodeId::Commit(id) if branch != MAIN => history::commit(&service.store, id)?
                .parent
                .map(NodeId::Commit),
            _ => break,
        };
        if seen < skip {
            seen += 1;
            continue;
        }
        if items.len() == page.limit {
            more = true;
            next_page = Some(node);
            break;
        }
        let record = saved::summary(&service.store, node)?;
        // Serialized size bounds the actual reply, not an optimistic per-record estimate.
        bytes += saved::reply_cost(&record);
        if bytes > 44 * 1024 {
            return Err(history::refuse(
                "Capacity",
                "saved history page exceeds reply budget; reduce page_size",
                SdkValue::Null,
            ));
        }
        items.push(record);
    }
    let mut next_key = key;
    next_key.page += 1;
    pages::keep(&service.store, next_key, next_page);
    Ok(SdkValue::map([
        ("branch", SdkValue::text(branch)),
        ("atCommit", saved::reference(&service.store, start)?),
        ("page", SdkValue::Int(page.number as i64)),
        ("pageSize", SdkValue::Int(page.limit as i64)),
        ("items", SdkValue::List(items)),
        ("hasMore", SdkValue::Bool(more)),
        (
            "scan",
            SdkValue::map([
                ("complete", SdkValue::Bool(true)),
                ("examined", SdkValue::Int(examined as i64)),
                ("reason", SdkValue::Null),
            ]),
        ),
    ]))
}
