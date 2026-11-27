//! `file.edit`.
use crate::families::args::{commit, single, Answer};
use crate::StoreService;
use layerfs_protocol::sdk::SdkValue;

/// `edit(path, { oldText, newText }, { branch })`: one Commit replacing the one occurrence of a text in a file.
pub fn edit(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "edit")])
}
