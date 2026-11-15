//! The `branch` family: 9 calls, one file each.
mod commits;
mod diff;
mod fork;
mod get;
mod get_commit;
mod list;
mod merge;
mod publish;
mod remove;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        list::CALL,
        get::CALL,
        commits::CALL,
        get_commit::CALL,
        fork::CALL,
        merge::CALL,
        publish::CALL,
        diff::CALL,
        remove::CALL,
    ]
}
