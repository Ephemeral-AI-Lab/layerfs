//! The `branch` family: 6 calls, one file each. Reading history is the
//! `commit` family's.
mod fork;
mod get;
mod list;
mod merge;
mod publish;
mod remove;

use crate::dispatch::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        list::CALL,
        get::CALL,
        fork::CALL,
        merge::CALL,
        publish::CALL,
        remove::CALL,
    ]
}
