//! `workspace.mount`: mount a Branch in a Sandbox.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::with_field;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::workspace::calls::{mount, MountRequest};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "mount",
    &[a("sandbox", Name), a("branch", Name)],
    &[opt("name", Name)],
    "mount a Branch in a Sandbox",
    run,
);

/// Runs `workspace.mount`: its side's answer with the Workspace's `ws`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = MountRequest {
        sandbox: text("sandbox").unwrap_or_default(),
        branch: text("branch"),
        name: text("name"),
    };
    let (home, events) = context.parts();
    let mounted = mount(home, events, &request)?;
    Ok(with_field(mounted.answer, "ws", SdkValue::Text(mounted.ws)))
}
