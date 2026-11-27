//! The `home` family: setting the home up on purpose (`init`), reading its
//! state and what this build would use with it without changing anything
//! (`get`), its call log (`log`, `clear_log`; #659), read and cleared in
//! the caller's process, and its doctor. `init` opens the home itself to
//! say what it did; `get` and `doctor` never open it. What earlier runs
//! left behind has no call: LayerFS removes it itself (spec v20).
mod clear_log;
mod doctor;
mod get;
mod init;
mod log;

use crate::dispatch::Calls;
pub(crate) use get::build;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        init::CALL,
        get::CALL,
        log::CALL,
        clear_log::CALL,
        doctor::CALL,
    ]
}
