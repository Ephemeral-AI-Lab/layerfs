//! `fs.apply`.
use crate::methods::args::{commit, invalid, Answer};
use crate::StoreService;
use layerfs_bridge::control::SdkValue;

/// `apply(changes[], { branch })`: one Commit for every change.
pub fn apply(service: &StoreService, args: &SdkValue) -> Answer {
    let Some(SdkValue::List(changes)) = args.get("changes") else {
        return Err(invalid("apply requires a changes array"));
    };
    commit(service, args, changes)
}
