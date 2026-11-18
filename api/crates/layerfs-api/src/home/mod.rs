//! The `home` family: setting the home up on purpose (`init`), reading its
//! state and what this build would use with it without changing anything
//! (`get`), and its call log (`log`, `clear_log`; #659), read and cleared
//! in the caller's process. `init` opens the home itself to say what it
//! did, and `get` never opens it.
mod clear_log;
mod get;
mod init;
mod log;

use crate::calls::Calls;
pub(crate) use get::build;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[init::CALL, get::CALL, log::CALL, clear_log::CALL]
}
