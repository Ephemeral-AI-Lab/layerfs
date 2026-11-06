//! The `branch` family: 6 calls, one file each.
mod diff;
mod fork;
mod get;
mod list;
mod merge;
mod publish;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        list::CALL,
        get::CALL,
        fork::CALL,
        merge::CALL,
        publish::CALL,
        diff::CALL,
    ]
}
