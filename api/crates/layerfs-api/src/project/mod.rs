//! The `project` family: 9 calls, one file each.
mod delete;
mod doctor;
mod export;
mod get;
mod hide_path;
mod init;
mod list;
mod prune;
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
        prune::CALL,
        hide_path::CALL,
        doctor::CALL,
    ]
}
