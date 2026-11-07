//! The `workspace` family: 6 calls, one file each (O14: status).
mod commit;
mod exec;
mod exec_cancel;
mod mount;
mod status;
mod unmount;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        mount::CALL,
        exec::CALL,
        exec_cancel::CALL,
        commit::CALL,
        unmount::CALL,
        status::CALL,
    ]
}
