//! The `sandbox` family: 4 calls, one file each.
mod close;
mod create;
mod get;
mod list;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[create::CALL, list::CALL, get::CALL, close::CALL]
}
