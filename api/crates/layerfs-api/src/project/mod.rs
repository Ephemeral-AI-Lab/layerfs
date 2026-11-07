//! The `project` family: 4 calls, one file each.
mod get;
mod init;
mod list;
mod remove;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[init::CALL, list::CALL, get::CALL, remove::CALL]
}
