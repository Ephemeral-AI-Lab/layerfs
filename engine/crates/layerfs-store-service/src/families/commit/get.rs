//! `commit.get`: one Commit of the project, by its id or a prefix.
use crate::families::args::{project, text, Answer};
use crate::{history::listing, StoreService};
use layerfs_protocol::sdk::SdkValue;

/// `get({ commit, branch? })`: the Commit, resolved in one read of
/// the identity index; with `branch`, only one that Branch's history holds.
pub fn get(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let commit = text(args, "commit")?;
    listing::commit(&service.store, &stack, commit, args.get_text("branch"))
}
