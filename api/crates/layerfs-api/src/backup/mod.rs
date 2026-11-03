//! The `backup` family: 5 calls, one file each.
mod connect;
mod create;
mod disconnect;
mod list;
mod restore;

use layerfs_control::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        connect::CALL,
        disconnect::CALL,
        list::CALL,
        create::CALL,
        restore::CALL,
    ]
}
