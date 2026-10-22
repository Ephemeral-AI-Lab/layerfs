//! `branch.list_commits`.
use crate::base::args::{paging, project, text, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::{
    history::{branch, lookup, MAIN},
    StoreService,
};

/// `list_commits({ branch, pageSize?, pagePos? })`: one page, newest first.
///
/// `main` names the stack's line of Layers, not a Branch: it has no Commits,
/// so it answers one empty page after the page arguments are read.
pub fn list_commits(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let wanted = text(args, "branch")?;
    let named = if wanted == MAIN {
        None
    } else {
        Some(lookup::branch(&service.store, &stack, wanted)?)
    };
    let (limit, pos) = paging(args)?;
    match named {
        Some(named) => branch::list_commits(&service.store, &named, limit, pos),
        None => Ok(empty_page()),
    }
}

/// The answer for a ref that holds no Commits.
fn empty_page() -> SdkValue {
    SdkValue::map([
        ("items", SdkValue::List(Vec::new())),
        ("hasMore", SdkValue::Bool(false)),
        ("nextPagePos", SdkValue::Null),
    ])
}
