//! `branch.publish`.
use crate::families::args::{project, text, Answer};
use crate::{history::branch, lookup, StoreService};
use layerfs_protocol::sdk::SdkValue;

/// `publish({ branch, at?, expected?, force? })`: one new main Layer holding
/// the Commit `at` (the Branch head when absent), refused `HeadMoved` when
/// main is not at `expected`, or moved since the Branch forked unless
/// `force` asks for the overwrite.
pub fn publish(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let named = lookup::branch(&service.store, &stack, text(args, "branch")?)?;
    branch::publish(
        &service.store,
        &stack,
        &named,
        args.get_text("at"),
        args.get_text("expected"),
        args.get_bool("force"),
    )
}
