//! The `home` family: setting the home up on purpose (`init`), reading its
//! state and what this build would use with it without changing anything
//! (`get`), its call log (`log`, `clear_log`; #659), read and cleared in
//! the caller's process, its doctor, and what earlier runs left behind
//! (`leftovers`) with the removal of one of them (`prune`). `init` opens
//! the home itself to say what it did; `get`, `doctor` and `leftovers`
//! never open it.
mod clear_log;
mod get;
mod init;
mod leftovers;
mod log;
mod prune;

use crate::calls::Calls;
pub(crate) use get::build;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        init::CALL,
        get::CALL,
        log::CALL,
        clear_log::CALL,
        crate::doctor::home::CALL,
        leftovers::CALL,
        prune::CALL,
    ]
}
