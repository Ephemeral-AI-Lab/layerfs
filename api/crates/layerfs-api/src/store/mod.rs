//! The `store` family: one file each (call surface 2.0). A sqlite Store
//! is created, moved between this computer and a machine, and removed; a
//! PostgreSQL database is connected and disconnected with its URL and
//! login, saved on this computer only, and machines attach to it to run
//! its Sandboxes (`attach`, `detach`). `store.get` answers from the
//! home's records (it absorbs `store.status` and `store.stats`), and
//! `store.move` replaces `store.push` and `store.pull`.
mod attach;
mod compact;
mod connect;
mod create;
mod detach;
mod disconnect;
mod doctor;
pub(crate) mod get;
mod list;
mod r#move;
mod remove;
mod repair_mount_holders;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        list::CALL,
        get::CALL,
        create::CALL,
        connect::CALL,
        disconnect::CALL,
        attach::CALL,
        detach::CALL,
        r#move::CALL,
        remove::CALL,
        compact::CALL,
        repair_mount_holders::CALL,
        doctor::CALL,
    ]
}
