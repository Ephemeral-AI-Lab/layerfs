//! `branch.publish`.
use crate::base::args::{project, text, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::{
    history::{branch, lookup},
    StoreService,
};

/// `publish({ branch, commit? })`: one new main Layer holding the Branch head, reporting the Layers it overwrote.
pub fn publish(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let named = lookup::branch(&service.store, &stack, text(args, "branch")?)?;
    let commit = args.get_text("commit").map(lookup::node).transpose()?;
    branch::publish(&service.store, &stack, &named, commit)
}
