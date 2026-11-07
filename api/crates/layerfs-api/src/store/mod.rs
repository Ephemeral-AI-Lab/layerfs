//! The `store` family: 9 calls, one file each (H1: push and pull replace move).
mod connect;
mod create;
mod disconnect;
mod get;
mod list;
mod pull;
mod push;
mod remove;
mod status;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        list::CALL,
        get::CALL,
        status::CALL,
        create::CALL,
        connect::CALL,
        push::CALL,
        pull::CALL,
        remove::CALL,
        disconnect::CALL,
    ]
}
