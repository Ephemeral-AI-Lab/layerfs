//! `branch.list_commits`.
use crate::methods::args::{paging, project, text, Answer};
use crate::{
    history::{branch, lookup, MAIN},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `list_commits({ branch, pageSize?, pagePos? })`: one page, newest first.
/// `main` lists the stack's published Layers (S §3.3). The Branch is
/// resolved before the page arguments are read, as before.
pub fn list_commits(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let named = text(args, "branch")?;
    if named == MAIN {
        let (limit, pos) = paging(args)?;
        return branch::list_layers(&service.store, &stack, limit, pos);
    }
    let branch = lookup::branch(&service.store, &stack, named)?;
    let (limit, pos) = paging(args)?;
    branch::list_commits(&service.store, &branch, limit, pos)
}
