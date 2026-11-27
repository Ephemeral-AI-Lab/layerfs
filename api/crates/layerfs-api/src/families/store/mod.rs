//! The `store` family: one file each (call surface 2.0). A sqlite Store
//! is created, moved between this computer and a machine, and removed; a
//! PostgreSQL database is connected and disconnected with its URL and
//! login, saved on this computer only, and machines attach to it to run
//! its Sandboxes (`attach`, `detach`). `store.get` answers from the
//! home's records (it absorbs `store.status` and `store.stats`), and
//! `store.move` replaces `store.push` and `store.pull`.
pub(crate) mod lifecycle;
pub(crate) mod maintenance;
pub(crate) mod status;

use crate::dispatch::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        status::list::CALL,
        status::get::CALL,
        lifecycle::create::CALL,
        lifecycle::connect::CALL,
        lifecycle::disconnect::CALL,
        lifecycle::attach::CALL,
        lifecycle::detach::CALL,
        lifecycle::r#move::CALL,
        lifecycle::remove::CALL,
        maintenance::compact::CALL,
        maintenance::upgrade::CALL,
        maintenance::repair_mount_holders::CALL,
        status::doctor::CALL,
    ]
}
