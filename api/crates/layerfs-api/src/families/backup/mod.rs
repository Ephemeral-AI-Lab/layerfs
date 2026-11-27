//! The `backup` family: 5 calls, one file each.
mod connect;
mod create;
mod disconnect;
mod doctor;
mod list;
mod restore;

use crate::dispatch::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        connect::CALL,
        disconnect::CALL,
        list::CALL,
        create::CALL,
        restore::CALL,
        doctor::CALL,
    ]
}
