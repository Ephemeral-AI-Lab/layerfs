//! `branch.diff_commits`.
use crate::families::args::{diff_page, project, text, Answer};
use crate::{
    filesystem::{compare::diff, root},
    StoreService,
};
use layerfs_protocol::sdk::SdkValue;

/// `diff_commits({ from, to })`: path-level changes between two Commits.
pub fn diff_commits(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let reader = root::reader(&service.store, &stack);
    let from = root::resolve(&service.store, &stack, &reader, text(args, "from")?)?;
    let to = root::resolve(&service.store, &stack, &reader, text(args, "to")?)?;
    diff::diff_commits(&service.store, &reader, &from, &to, diff_page(args))
}
