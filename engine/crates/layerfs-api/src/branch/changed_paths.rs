//! `branch.changed_paths`.
use crate::base::args::{paging, project, text, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::{
    read::{compare, snapshot},
    StoreService,
};

/// `changed_paths({ commit, pageSize?, pagePos? })`: one Commit's path changes.
pub fn changed_paths(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let reader = snapshot::reader(&service.store, &stack);
    let commit = text(args, "commit")?;
    let (limit, pos) = paging(args)?;
    compare::changed_paths(
        &service.store,
        &stack,
        &reader,
        commit,
        Ok(compare::Request {
            limit,
            pos,
            cursor: None,
            hunks: false,
        }),
    )
}
