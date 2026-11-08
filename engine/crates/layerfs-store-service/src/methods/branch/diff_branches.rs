//! `branch.diff_branches`.
use crate::methods::args::{diff_page, project, text, Answer};
use crate::{
    read::{compare, snapshot},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `diff_branches({ from, to })`: path-level changes between two Branches or main.
pub fn diff_branches(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let extended = [
        "page",
        "fromCommit",
        "toCommit",
        "pathPrefix",
        "includeAttribution",
    ]
    .iter()
    .any(|key| args.get(key).is_some_and(|v| !matches!(v, SdkValue::Null)));
    if extended {
        let reader = snapshot::reader(&service.store, &stack);
        let from_node = crate::history::saved::resolve_context(
            &service.store,
            &stack,
            text(args, "from")?,
            args.get_text("fromCommit"),
        )?;
        let to_node = crate::history::saved::resolve_context(
            &service.store,
            &stack,
            text(args, "to")?,
            args.get_text("toCommit"),
        )?;
        let from = snapshot::resolve(
            &service.store,
            &stack,
            &reader,
            &crate::history::lookup::node_text(from_node),
        )?;
        let to = snapshot::resolve(
            &service.store,
            &stack,
            &reader,
            &crate::history::lookup::node_text(to_node),
        )?;
        return crate::read::branch_compare::diff(&service.store, &reader, &from, &to, args);
    }
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
