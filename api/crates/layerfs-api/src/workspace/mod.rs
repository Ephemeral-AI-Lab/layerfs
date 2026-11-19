//! The `workspace` family: 7 calls, one file each (call surface 2.0).
mod cancel;
mod commit;
mod exec;
mod list;
mod mount;
mod status;
mod unmount;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        list::CALL,
        mount::CALL,
        exec::CALL,
        cancel::CALL,
        commit::CALL,
        unmount::CALL,
        status::CALL,
    ]
}
