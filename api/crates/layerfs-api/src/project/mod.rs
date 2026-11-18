//! The `project` family: 7 calls, one file each.
mod delete;
mod export;
mod get;
mod init;
mod list;
mod remove_file;
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
        remove_file::CALL,
        crate::doctor::project::CALL,
    ]
}
