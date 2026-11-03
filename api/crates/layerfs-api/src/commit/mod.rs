//! The `commit` family: 3 calls, one file each.
mod diff;
mod get;
mod list;

use layerfs_control::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[list::CALL, get::CALL, diff::CALL]
}
