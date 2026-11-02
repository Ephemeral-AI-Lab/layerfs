//! `fs.edit`.
use crate::methods::args::{commit, single, Answer};
use crate::StoreService;
use layerfs_bridge::control::SdkValue;

/// `edit(path, { oldText, newText }, { branch })`: one Commit replacing the one occurrence of a text in a file.
pub fn edit(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "edit")])
}
