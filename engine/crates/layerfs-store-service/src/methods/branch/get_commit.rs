//! One saved record admitted through a branch's saved history.
use crate::{
    history::saved,
    methods::args::{project, text, Answer},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// Full immutable IDs only; publication source remains a separate relationship.
pub fn get_commit(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let node = saved::resolve_context(
        &service.store,
        &stack,
        text(args, "branch")?,
        Some(text(args, "commit")?),
    )?;
    let record = saved::record(&service.store, node)?;
    if saved::reply_cost(&record) > crate::read::window::BYTES - 512 {
        return Err(crate::read::query::capacity(
            "saved record details exceed reply budget; use branch.commits for its compact summary",
        ));
    }
    Ok(record)
}
