//! `branch.diff_branches`.
use crate::base::args::{diff_page, project, text, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::{
    read::{compare, snapshot},
    StoreService,
};

/// `diff_branches({ from, to })`: path-level changes between two Branches or main.
pub fn diff_branches(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let reader = snapshot::reader(&service.store, &stack);
    let from_name = text(args, "from")?;
    let from = snapshot::resolve(&service.store, &stack, &reader, from_name)?;
    let to_name = text(args, "to")?;
    let to = snapshot::resolve(&service.store, &stack, &reader, to_name)?;
    compare::diff_branches(
        &service.store,
        &reader,
        (&from, from_name),
        (&to, to_name),
        diff_page(args),
    )
}
