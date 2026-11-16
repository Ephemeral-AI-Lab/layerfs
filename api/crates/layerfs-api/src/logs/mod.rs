//! The `logs` family: the home's call log, read and cleared in the
//! caller's process (#659), and the logs of a machine's worker and a
//! Sandbox's daemon, read by cursor where they are written
//! (layerfs-project#178). No call here starts a machine or writes a
//! call-log line.
mod list;
mod machine;
mod reset;
mod sandbox;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[list::CALL, reset::CALL, machine::CALL, sandbox::CALL]
}
