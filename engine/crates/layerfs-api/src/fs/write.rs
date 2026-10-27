//! `fs.write`.
use crate::base::args::{commit, single, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::StoreService;

/// `write(path, content, { branch })`: one Commit writing a file.
pub fn write(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "write")])
}
