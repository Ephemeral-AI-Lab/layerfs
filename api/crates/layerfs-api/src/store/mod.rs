//! The `store` family: one file each (call surface 2.0). A sqlite Store
//! is created, moved between this computer and a machine, and removed; a
//! PostgreSQL database is connected and disconnected, with an address of
//! its own per machine (`connect --machine`), and names a saved login
//! (`save_auth`, `auths`, `remove_auth`). `store.get` answers from the
//! home's records (it absorbs `store.status` and `store.stats`), and
//! `store.move` replaces `store.push` and `store.pull`.
mod auths;
mod compact;
mod connect;
mod create;
mod disconnect;
mod get;
mod list;
mod r#move;
mod remove;
mod remove_auth;
mod save_auth;
mod update;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        list::CALL,
        get::CALL,
        create::CALL,
        connect::CALL,
        disconnect::CALL,
        save_auth::CALL,
        auths::CALL,
        remove_auth::CALL,
        update::CALL,
        r#move::CALL,
        remove::CALL,
        compact::CALL,
        crate::doctor::store::CALL,
    ]
}
