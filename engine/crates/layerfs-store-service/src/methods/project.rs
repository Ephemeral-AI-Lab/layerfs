//! `project.list_branches`.
use crate::methods::args::{invalid, paging, project, Answer};
use crate::{history::branch, StoreService};
use layerfs_bridge::control::SdkValue;

/// `list_branches({ name?, pageSize?, pagePos? })`: every Branch with its id, base and head, and main, in name order.
pub fn list_branches(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let (limit, pos) = paging(args)?;
    let name = match args.get("name") {
        None => None,
        Some(SdkValue::Text(name)) => Some(name.as_str()),
        Some(_) => return Err(invalid("name must be text")),
    };
    branch::list(&service.store, &stack, limit, pos, name)
}
