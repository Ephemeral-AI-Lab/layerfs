//! `fs.rm`.
use crate::methods::args::{commit, single, Answer};
use crate::StoreService;
use layerfs_protocol::sdk::SdkValue;

/// `rm(path, { branch })`: one Commit removing a path, recursively.
pub fn rm(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "rm")])
}
