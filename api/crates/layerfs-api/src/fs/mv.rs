//! `fs.mv`.
use crate::base::args::{commit, single, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::StoreService;

/// `mv(from, to, { branch })`: one Commit moving a path.
pub fn mv(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "mv")])
}
