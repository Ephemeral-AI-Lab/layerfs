//! `branch.fork`.
use crate::methods::args::{project, text, Answer};
use crate::{history::branch, StoreService};
use layerfs_bridge::control::SdkValue;

/// `fork(name, { from })`: a new Branch at a Branch, main, or a Commit or
/// Layer id; metadata only.
pub fn fork(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let name = text(args, "name")?;
    branch::fork(&service.store, &stack, name, args.get_text("from"))
}
