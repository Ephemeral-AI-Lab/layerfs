//! The `logs` family: the logs of a machine's worker and a Sandbox's
//! daemon, read by cursor where they are written (layerfs-project#178). No
//! call here starts a machine or writes a call-log line. (The home's call
//! log is `home.log` and `home.clear_log` since the call surface 2.0.)
mod machine;
mod sandbox;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[machine::CALL, sandbox::CALL]
}
