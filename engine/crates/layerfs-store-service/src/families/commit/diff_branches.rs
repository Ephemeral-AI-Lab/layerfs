//! `branch.diff_branches`.
use crate::families::args::{diff_page, project, text, Answer};
use crate::{
    filesystem::{compare::diff, root},
    StoreService,
};
use layerfs_protocol::sdk::SdkValue;

/// `diff_branches({ from, to })`: path-level changes between two Branches or main.
pub fn diff_branches(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let extended = [
        "page",
        "fromCommit",
        "toCommit",
        "pathPrefix",
        "includeAttribution",
        "continuation",
    ]
    .iter()
    .any(|key| args.get(key).is_some_and(|v| !matches!(v, SdkValue::Null)));
    if extended {
        let reader = root::reader(&service.store, &stack);
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
        let from = root::resolve(
            &service.store,
            &stack,
            &reader,
            &layerfs_protocol::sdk::node_text(from_node),
        )?;
        let to = root::resolve(
            &service.store,
            &stack,
            &reader,
            &layerfs_protocol::sdk::node_text(to_node),
        )?;
        return crate::filesystem::compare::tree_pages::diff(
            &service.store,
            &reader,
            &from,
            &to,
            args,
        );
    }
    let reader = root::reader(&service.store, &stack);
    let from_name = text(args, "from")?;
    let from = root::resolve(&service.store, &stack, &reader, from_name)?;
    let to_name = text(args, "to")?;
    let to = root::resolve(&service.store, &stack, &reader, to_name)?;
    diff::diff_branches(
        &service.store,
        &reader,
        (&from, from_name),
        (&to, to_name),
        diff_page(args),
    )
}
