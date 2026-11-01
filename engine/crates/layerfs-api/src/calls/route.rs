//! Each call of the table to its family's function. A call whose function
//! lands in a later #533 step answers `Unsupported`, naming the step, before
//! any effect.
use super::dispatch::Context;
use super::table::Call;
use crate::host::outcome::{refused, Outcome};
use layerfs_bridge::control::SdkValue;

/// One family function: the running call and its checked arguments.
pub type Handler = fn(&mut Context<'_>, &SdkValue) -> Outcome;

/// Runs `call`'s function.
pub fn run(call: &Call, context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    if matches!(call.family, "branch" | "commit" | "file") {
        return crate::family::files::run(context, call.family, call.call, args);
    }
    match handler(call) {
        Ok(handler) => handler(context, args),
        Err(step) => Err(refused(
            "Unsupported",
            format!("{} is not built yet (#533 step {step})", call.name()),
        )),
    }
}

/// The function of `call`, or the #533 step that builds it.
pub fn handler(call: &Call) -> Result<Handler, u8> {
    Err(match call.family {
        "project" => 9,
        "sandbox" | "workspace" => 12,
        "machine" => 6,
        "store" if matches!(call.call, "connect" | "disconnect") => 10,
        "store" => 8,
        "backup" => 11,
        _ => 2,
    })
}
