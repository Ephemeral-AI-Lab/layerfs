//! `fs.diff_files`.
use crate::methods::args::{project, selector, Answer};
use crate::{read::compare, StoreService};
use layerfs_bridge::control::SdkValue;

/// `diff_files({ left, right }, { branch })`: two files compared line by line.
pub fn diff_files(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let branch = args.get_text("branch").unwrap_or("main");
    compare::diff_files(
        &service.store,
        &stack,
        selector(args, "left", branch),
        selector(args, "right", branch),
    )
}
