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
    if call.family == "backup" {
        return crate::family::backups::run(context, call.call, args);
    }
    if call.family == "project" {
        return crate::family::projects::run(context, call.call, args);
    }
    if call.family == "store" {
        if matches!(call.call, "connect" | "disconnect") {
            return crate::family::postgres::run(context, call.call, args);
        }
        return crate::family::stores::run(context, call.call, args);
    }
    #[cfg(feature = "machine")]
    if call.family == "machine" {
        use crate::machine as m;
        return match call.call {
            "list" => m::list(context, args),
            "get" => m::get(context, args),
            "status" => m::status(context, args),
            "create" => m::create(context, args),
            "connect" => m::connect(context, args),
            "remove" => m::remove(context, args),
            "disconnect" => m::disconnect(context, args),
            _ => m::update(context, args),
        };
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
        "sandbox" | "workspace" => 12,
        "machine" => 6,
        _ => 2,
    })
}
