//! The `workspace` family: 5 calls, one file each (O14: status).
mod commit;
mod exec;
mod mount;
mod status;
mod unmount;

use crate::calls::Calls;
use layerfs_bridge::control::SdkValue;
use layerfs_sandbox::host::Message;

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

/// The Workspace a call names (`ws`).
fn ws(args: &SdkValue) -> String {
    args.get_text("ws").unwrap_or("").to_owned()
}

/// The Commit message a call carries: null when it has none.
fn message(args: &SdkValue) -> Message {
    match Message::read(args.get("message")) {
        Message::Absent => Message::Null,
        message => message,
    }
}

/// Flag `name` as sent: `None` when not sent, else whether it is `true`.
fn flag(args: &SdkValue, name: &str) -> Option<bool> {
    args.get(name).map(|_| args.get_bool(name))
}
