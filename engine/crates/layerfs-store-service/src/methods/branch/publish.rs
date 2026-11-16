//! `branch.publish`.
use crate::methods::args::{project, text, Answer};
use crate::{
    history::{branch, lookup},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `publish({ branch, commit? })`: one new main Layer holding the Branch head, reporting the Layers it overwrote.
pub fn publish(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let named = lookup::branch(&service.store, &stack, text(args, "branch")?)?;
    let commit = args.get_text("commit").map(lookup::node).transpose()?;
    branch::publish(&service.store, &stack, &named, commit)
}
