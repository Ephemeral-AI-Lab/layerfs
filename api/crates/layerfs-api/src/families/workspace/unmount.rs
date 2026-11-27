//! `workspace.unmount`: end a Workspace.
use crate::dispatch::guard::scope::Rule;
use crate::dispatch::table::Kind::{Bool, Text};
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Form};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_workspace::answers::unmounted;
use layerfs_workspace::route::send::{unmount, UnmountRequest};

const FORM: Form = Form::of(
    &["ok"],
    &[],
    &["ok", "outcome", "abort", "detach", "cleanup"],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "unmount",
    &[a("workspace", Text)],
    &[opt("discard", Bool), opt("force", Bool)],
    "end a Workspace",
    run,
)
.form(&FORM)
.scoped(Rule::Workspace);

/// Runs `workspace.unmount`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = UnmountRequest {
        ws: args.get_text("workspace").unwrap_or("").to_owned(),
        discard: args.get("discard").map(|_| args.get_bool("discard")),
        force: args.get("force").map(|_| args.get_bool("force")),
    };
    let (home, events) = context.parts();
    Ok(unmounted(&unmount(home, events, &request)?))
}
