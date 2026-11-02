//! `fs.edit`.
use crate::base::args::{commit, single, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::StoreService;

/// `edit(path, { oldText, newText }, { branch })`: one Commit replacing the one occurrence of a text in a file.
pub fn edit(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "edit")])
}
