//! `branch.fork`.
use crate::methods::args::{project, text, Answer};
use crate::{history::branch, StoreService};
use layerfs_protocol::provision::install::SdkValue;

/// `fork(name, { from?, at? })`: a new Branch at the head of `from` (a
/// Branch name first, else a Commit id or prefix; main when absent), or at
/// the Commit `at` of its history; metadata only.
pub fn fork(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let name = text(args, "name")?;
    branch::fork(
        &service.store,
        &stack,
        name,
        args.get_text("from"),
        args.get_text("at"),
    )
}
