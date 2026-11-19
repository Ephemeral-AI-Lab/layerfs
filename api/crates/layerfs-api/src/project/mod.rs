//! The `project` family: 7 calls, one file each.
mod delete;
mod doctor;
mod export;
mod get;
mod hide_path;
mod init;
mod list;
mod stats;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        init::CALL,
        list::CALL,
        get::CALL,
        stats::CALL,
        export::CALL,
        delete::CALL,
        hide_path::CALL,
        doctor::CALL,
    ]
}
