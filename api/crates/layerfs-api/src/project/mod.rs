//! The `project` family: 7 calls, one file each.
mod export;
mod get;
mod init;
mod list;
mod remove;
mod stats;
mod storage_report;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        init::CALL,
        list::CALL,
        get::CALL,
        stats::CALL,
        storage_report::CALL,
        export::CALL,
        remove::CALL,
    ]
}
