//! `branch.diff_commits`.
use crate::base::args::{diff_page, project, text, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::{
    read::{compare, snapshot},
    StoreService,
};

/// `diff_commits({ from, to })`: path-level changes between two Commits.
pub fn diff_commits(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let reader = snapshot::reader(&service.store, &stack);
    let from = snapshot::resolve(&service.store, &stack, &reader, text(args, "from")?)?;
    let to = snapshot::resolve(&service.store, &stack, &reader, text(args, "to")?)?;
    compare::diff_commits(&service.store, &reader, &from, &to, diff_page(args))
}
