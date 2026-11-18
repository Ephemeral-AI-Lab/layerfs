//! The `logs` family: a Sandbox's daemon log, read by cursor where it is
//! written (layerfs-project#178). No call here starts a machine or writes
//! a call-log line. (The home's call log is `home.log` and
//! `home.clear_log`, a machine's worker log `machine.log`, since the call
//! surface 2.0.)
mod sandbox;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[sandbox::CALL]
}
