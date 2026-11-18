//! The `commit` family: 3 calls, one file each; the only place history is
//! read.
mod diff;
mod get;
mod list;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[list::CALL, get::CALL, diff::CALL]
}
