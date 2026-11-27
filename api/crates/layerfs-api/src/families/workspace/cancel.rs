//! `workspace.cancel`: end the commands running in a Workspace, or one of
//! them (was `workspace.exec_cancel`).
use crate::dispatch::guard::scope::Rule;
use crate::dispatch::table::Kind::Text;
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Form};
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_workspace::exec::call::{cancel_call, CancelRequest};

/// One command's cancel.
const COMMAND: Form = Form::of(&[], &[], &["exec", "outcome", "signal"]);
const FORM: Form = Form::of(
    &[],
    &["signal", "already_ended", "unknown"],
    &["signal", "already_ended", "unknown", "outcome", "commands"],
)
.parts(&[("commands", crate::dispatch::Part::Rows(&COMMAND))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "cancel",
    &[a("workspace", Text)],
    &[opt("exec", Text)],
    "end the commands running in a Workspace, or one of them",
    run,
)
.form(&FORM)
.agent()
.scoped(Rule::Workspace);

/// Runs `workspace.cancel`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = CancelRequest {
        ws: args.get_text("workspace").unwrap_or("").to_owned(),
        exec: args.get_text("exec").map(str::to_owned),
    };
    let (home, events) = context.parts();
    cancel_call(home, events, &request)
}
