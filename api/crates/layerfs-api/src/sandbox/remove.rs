//! `sandbox.remove`: stop a Sandbox and remove it (`sandbox.close` before
//! the call surface 2.0).
use crate::calls::table::Kind::{Bool, Name};
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::Context;
use crate::calls::Form;
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::family::{remove, RemoveRequest, Removed};

/// Its answer: `ok`, the Workspaces a forced remove ended, and what may be
/// left unrecorded.
const FORM: Form = Form::of(
    &[],
    &["ok", "unmounted", "left"],
    &["ok", "unmounted", "left", "outcome", "sandbox"],
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

/// Runs `sandbox.remove`: `outcome` `removed` or `already_gone` (its runtime
/// was gone, it never recorded one, or it was a Sandbox of the retired
/// docker backend, whose container `left` names).
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
    let (outcome, unmounted, left) = match remove(home, events, &request)? {
        Removed::Stopped { unmounted, left } => ("removed", unmounted, left),
        Removed::AlreadyGone { left } => ("already_gone", Vec::new(), left),
    };
    Ok(SdkValue::map([
        ("unmounted", texts(unmounted)),
        ("left", texts(left)),
        ("outcome", SdkValue::text(outcome)),
        ("sandbox", SdkValue::text(&request.name)),
    ]))
}
