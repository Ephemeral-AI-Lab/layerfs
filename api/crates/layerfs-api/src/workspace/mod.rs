//! The `workspace` family: 5 calls, one file each (O14: status).
mod commit;
mod exec;
mod mount;
mod status;
mod unmount;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        mount::CALL,
        exec::CALL,
        commit::CALL,
        unmount::CALL,
        status::CALL,
    ]
}
