//! The `step` family (#555): 4 calls, one file each.
mod cancel;
mod exec_async;
mod exec_status;
mod exec_wait;

use layerfs_control::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        exec_async::CALL,
        exec_wait::CALL,
        exec_status::CALL,
        cancel::CALL,
    ]
}
