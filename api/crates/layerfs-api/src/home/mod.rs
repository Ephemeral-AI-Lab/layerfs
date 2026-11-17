//! The `home` family: setting the home up on purpose (`init`) and reading
//! what this build would use with it without changing anything (`get`).
//! Neither is opened by the dispatcher: `init` opens the home itself to say
//! what it did, and `get` never opens it.
mod get;
mod init;

use crate::calls::Calls;
pub(crate) use get::build;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[init::CALL, get::CALL]
}
