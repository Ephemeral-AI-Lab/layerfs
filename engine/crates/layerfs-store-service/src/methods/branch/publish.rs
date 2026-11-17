//! `branch.publish`.
use crate::methods::args::{project, text, Answer};
use crate::{
    history::{branch, lookup},
    StoreService,
};
use layerfs_bridge::control::SdkValue;
use layerfs_history::NodeId;

/// `publish({ branch, commit?, expectedBase?, expectedHead?, force? })`: one new
/// main Layer holding the Branch head, refused `HeadMoved` when main moved
/// since the Branch forked unless `force` asks for the overwrite.
pub fn publish(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let named = lookup::branch(&service.store, &stack, text(args, "branch")?)?;
    let commit = args.get_text("commit").map(lookup::node).transpose()?;
    let expected_base = match args.get_text("expectedBase") {
        None => None,
        Some(base) => match lookup::node(base)? {
            NodeId::Layer(layer) => Some(layer),
            NodeId::Commit(_) => {
                return Err(lookup::invalid(
                    "expected_base must name a Layer, not a Commit",
                ))
            }
        },
    };
    let expected_head = match args.get_text("expectedHead") {
        None => None,
        Some(head) => match lookup::node(head)? {
            NodeId::Commit(commit) => Some(commit),
            NodeId::Layer(_) => {
                return Err(lookup::invalid(
                    "expected_head must name a Commit, not a Layer",
                ))
            }
        },
    };
    branch::publish(
        &service.store,
        &stack,
        &named,
        commit,
        expected_base,
        expected_head,
        args.get_bool("force"),
    )
}
