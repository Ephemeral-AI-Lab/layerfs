//! The `sandbox` family: 7 calls, one file each (call surface 2.0;
//! `sandbox.close` is `sandbox remove`, `logs.sandbox` is `sandbox log`, and
//! `sandbox doctor` is the doctor family's). `get` also holds the small
//! helpers that answer a Sandbox's state and Workspaces under the rows'
//! names.
mod create;
mod doctor;
pub(crate) mod files;
mod get;
mod list;
pub(crate) mod log;
mod remove;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        create::CALL,
        list::CALL,
        get::CALL,
        remove::CALL,
        files::CALL,
        log::CALL,
        doctor::CALL,
    ]
}
