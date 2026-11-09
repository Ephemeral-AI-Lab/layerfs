//! The `project` family: 6 calls, one file each.
mod export;
mod get;
mod init;
mod list;
mod remove;
mod stats;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        init::CALL,
        list::CALL,
        get::CALL,
        stats::CALL,
        export::CALL,
        remove::CALL,
    ]
}
