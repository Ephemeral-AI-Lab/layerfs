//! The `store` family: 13 calls, one file each (H1: push and pull replace
//! move; attach and detach supersede #536 C4).
mod attach;
mod connect;
mod create;
mod detach;
mod disconnect;
mod get;
mod list;
mod pull;
mod push;
mod remove;
mod stats;
mod status;
mod storage_report;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        list::CALL,
        get::CALL,
        status::CALL,
        stats::CALL,
        storage_report::CALL,
        create::CALL,
        connect::CALL,
        attach::CALL,
        detach::CALL,
        push::CALL,
        pull::CALL,
        remove::CALL,
        disconnect::CALL,
    ]
}
