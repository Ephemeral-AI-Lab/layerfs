//! The `project` family: 8 calls, one file each.
mod delete;
mod doctor;
mod export;
mod get;
mod hide_path;
mod init;
mod list;
mod stats;

use crate::dispatch::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        init::CALL,
        list::CALL,
        get::CALL,
        stats::CALL,
        doctor::CALL,
        export::CALL,
        delete::CALL,
        hide_path::CALL,
    ]
}
