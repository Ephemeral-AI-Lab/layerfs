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
    saved::record(&service.store, node)
}
