//! `sandbox.destroy`: delete only a Sandbox's data plane.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::with_field;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::family::{destroy, DestroyRequest, Destroyed};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "destroy",
    &[a("name", Name)],
    &[],
    "delete only a Sandbox's data plane",
    run,
);

/// Runs `sandbox.destroy`: `{ destroyed, ... }`, with how it ended.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = DestroyRequest {
        name: args.get_text("name").unwrap_or("").to_owned(),
    };
    let (home, events) = context.parts();
    let name = SdkValue::text(&request.name);
    match destroy(home, events, &request)? {
        Destroyed::Stopped(answer) => Ok(with_field(answer, "destroyed", name)),
        Destroyed::WasGone => Ok(SdkValue::map([
            ("destroyed", name),
            ("was_gone", SdkValue::Bool(true)),
        ])),
    }
}
