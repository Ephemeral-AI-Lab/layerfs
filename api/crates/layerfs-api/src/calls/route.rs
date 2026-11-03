//! Each call of the table to its family's function. A call whose function
//! lands in a later #533 step answers `Unsupported`, naming the step, before
//! any effect.
use super::dispatch::Context;
use super::table::Call;
use layerfs_bridge::control::SdkValue;
use layerfs_control::common::outcome::{refused, Outcome};

/// One family function: the running call and its checked arguments.
pub type Handler = fn(&mut Context<'_>, &SdkValue) -> Outcome;

/// Runs `call`'s function.
pub fn run(call: &Call, context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    if matches!(call.family, "branch" | "commit" | "file") {
        return layerfs_control::store::calls::run(context, call.family, call.call, args);
    }
    if call.family == "sandbox" {
        return layerfs_control::sandbox::family::sandbox(context, call.call, args);
    }
    if call.family == "workspace" {
        return layerfs_control::sandbox::family::workspace(context, call.call, args);
    }
    if call.family == "backup" {
        return layerfs_control::backup::family::run(context, call.call, args);
    }
    if call.family == "project" {
        return layerfs_control::project::family::run(context, call.call, args);
    }
    if call.family == "store" {
        if matches!(call.call, "connect" | "disconnect") {
            return layerfs_control::store::connect::run(context, call.call, args);
        }
        return layerfs_control::store::family::run(context, call.call, args);
    }
    #[cfg(feature = "machine")]
    if call.family == "machine" {
        use crate::machine as m;
        return match call.call {
            "list" => m::list(context, args),
            "platforms" => m::platforms(context, args),
            "get" => m::get(context, args),
            "status" => m::status(context, args),
            "start" => m::start(context, args),
            "stop" => m::stop(context, args),
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
        "machine" => 6,
        _ => 2,
    })
}
