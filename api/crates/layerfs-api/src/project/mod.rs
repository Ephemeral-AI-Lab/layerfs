//! The `project` family: 5 calls, one file each.
mod export;
mod get;
mod init;
mod list;
mod remove;

use layerfs_control::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        init::CALL,
        list::CALL,
        get::CALL,
        export::CALL,
        remove::CALL,
    ]
}
