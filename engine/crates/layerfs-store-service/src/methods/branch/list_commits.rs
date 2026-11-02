//! `branch.list_commits`.
use crate::methods::args::{paging, project, text, Answer};
use crate::{
    history::{branch, lookup},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `list_commits({ branch, pageSize?, pagePos? })`: one page, newest first.
pub fn list_commits(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let named = lookup::branch(&service.store, &stack, text(args, "branch")?)?;
    let (limit, pos) = paging(args)?;
    branch::list_commits(&service.store, &named, limit, pos)
}
