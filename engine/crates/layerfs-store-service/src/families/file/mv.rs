//! `file.mv`.
use crate::families::args::{commit, single, Answer};
use crate::StoreService;
use layerfs_protocol::sdk::SdkValue;

/// `mv(from, to, { branch })`: one Commit moving a path.
pub fn mv(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "mv")])
}
