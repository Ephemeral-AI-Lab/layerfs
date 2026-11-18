//! The `store` family: one file per call (H1: push and pull replace move;
//! a PostgreSQL Store runs on every machine, #536 C4, and `connect
//! --machine` gives one its own address; its saved logins are
//! `save_auth`, `auths` and `remove_auth`).
mod auths;
mod compact;
mod connect;
mod create;
mod disconnect;
mod get;
mod list;
mod pull;
mod push;
mod remove;
mod remove_auth;
mod save_auth;
mod stats;
mod status;
mod update;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        list::CALL,
        get::CALL,
        status::CALL,
        stats::CALL,
        create::CALL,
        connect::CALL,
        disconnect::CALL,
        save_auth::CALL,
        auths::CALL,
        remove_auth::CALL,
        update::CALL,
        push::CALL,
        pull::CALL,
        remove::CALL,
        compact::CALL,
        crate::doctor::store::CALL,
    ]
}
