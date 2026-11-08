//! `project.list_branches`.
use crate::methods::args::{invalid, paging, project, Answer};
use crate::{history::branch, StoreService};
use layerfs_bridge::control::SdkValue;

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
