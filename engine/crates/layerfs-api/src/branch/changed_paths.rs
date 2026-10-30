//! `branch.changed_paths`.
use crate::base::args::{diff_page, project, text, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::{
    read::{compare, snapshot},
    StoreService,
};

/// `changed_paths({ commit, pageSize?, pagePos?, cursor? })`: one Commit's path changes.
pub fn changed_paths(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let reader = snapshot::reader(&service.store, &stack);
    let commit = text(args, "commit")?;
    compare::changed_paths(&service.store, &stack, &reader, commit, diff_page(args))
}
