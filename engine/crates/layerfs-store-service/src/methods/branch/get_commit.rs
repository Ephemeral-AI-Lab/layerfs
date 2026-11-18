//! `branch.get_commit`: one Commit of the project, by its id or a prefix.
use crate::methods::args::{project, text, Answer};
use crate::{history::listing, StoreService};
use layerfs_bridge::control::SdkValue;

/// `get_commit({ commit, branch? })`: the Commit, resolved in one read of
/// the identity index; with `branch`, only one that Branch's history holds.
pub fn get_commit(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let commit = text(args, "commit")?;
    listing::commit(&service.store, &stack, commit, args.get_text("branch"))
}
