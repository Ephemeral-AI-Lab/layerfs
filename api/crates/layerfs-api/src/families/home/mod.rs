//! The `home` family: setting the home up on purpose (`init`), reading its
//! state and what this build would use with it without changing anything
//! (`get`), its call log (`log`, `clear_log`; #659), read and cleared in
//! the caller's process, and its doctor. `init` opens the home itself to
//! say what it did; `get` and `doctor` never open it. What earlier runs
//! left behind has no call: LayerFS removes it itself (spec v20).

pub(crate) use super::families::home::get::build;
use crate::dispatch::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        super::families::home::init::CALL,
        super::families::home::get::CALL,
        super::families::home::log::CALL,
        super::families::home::clear_log::CALL,
        super::families::home::doctor::CALL,
    ]
}
