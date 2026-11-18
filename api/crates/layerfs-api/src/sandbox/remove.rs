//! `sandbox.remove`: stop a Sandbox and remove it (`sandbox.close` before
//! the call surface 2.0).
use crate::calls::table::Kind::{Bool, Name};
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::Context;
use crate::calls::Form;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::family::{remove, RemoveRequest, Removed};

/// Its answer: `ok`, the Workspaces a forced remove ended, or a retired
/// Sandbox's container with the command that removes it.
const FORM: Form = Form::of(
    &[],
    &["ok", "unmounted", "container", "fix", "left"],
    &[
        "ok",
        "unmounted",
        "container",
        "fix",
        "left",
        "outcome",
        "sandbox",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "remove",
    &[a("sandbox", Name)],
    &[opt("force", Bool), PROGRESS],
    "stop a Sandbox and remove it",
    run,
)
.form(&FORM);

/// Runs `sandbox.remove`: `outcome` `removed`, `already_gone` (its runtime
/// was gone, or it never recorded one) or `forgotten` (a Sandbox of the
/// retired docker backend, whose container is left: `fix` removes it).
/// Without `--force` it is refused `Busy` while a command runs (with
/// `running`) and `InUse` while a Workspace is mounted (with `workspaces`).
/// Progress events are its phases: `cancel` (forced, one per command),
/// `stop` and `delete`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = RemoveRequest {
        name: args.get_text("sandbox").unwrap_or("").to_owned(),
        force: args.get_bool("force"),
    };
    let (home, events) = context.parts();
    let texts =
        |items: Vec<String>| SdkValue::List(items.into_iter().map(SdkValue::Text).collect());
    let (outcome, unmounted, container, left) = match remove(home, events, &request)? {
        Removed::Stopped { unmounted } => ("removed", unmounted, None, Vec::new()),
        Removed::AlreadyGone { left } => ("already_gone", Vec::new(), None, left),
        Removed::Forgotten { container, left } => ("forgotten", Vec::new(), Some(container), left),
    };
    let fix = (container.as_ref()).map(|container| format!("docker rm -f {container}"));
    Ok(SdkValue::map([
        ("unmounted", texts(unmounted)),
        ("container", SdkValue::opt_text(container)),
        ("fix", SdkValue::opt_text(fix)),
        ("left", texts(left)),
        ("outcome", SdkValue::text(outcome)),
        ("sandbox", SdkValue::text(&request.name)),
    ]))
}
