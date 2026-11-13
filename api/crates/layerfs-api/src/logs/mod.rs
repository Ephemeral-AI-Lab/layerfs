//! The `logs` family (#659): the home's call log, read and cleared in the
//! caller's process. Neither call starts a machine or writes a line.
mod list;
mod reset;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[list::CALL, reset::CALL]
}
