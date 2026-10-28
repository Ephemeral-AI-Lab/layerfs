//! `fs.apply`.
use crate::base::args::{commit, invalid, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::StoreService;

/// `apply(changes[], { branch })`: one Commit for every change.
pub fn apply(service: &StoreService, args: &SdkValue) -> Answer {
    let Some(SdkValue::List(changes)) = args.get("changes") else {
        return Err(invalid("apply requires a changes array"));
    };
    commit(service, args, changes)
}
