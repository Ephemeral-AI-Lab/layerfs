//! `workspace.mount`: mount a Branch in a Sandbox.
use crate::dispatch::guard::scope::Rule;
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Form, Policy};
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_workspace::answers::mounted;
use layerfs_workspace::route::send::{mount, MountRequest};

const FORM: Form = Form::of(
    &["workspace", "path"],
    &[],
    &[
        "workspace",
        "path",
        "outcome",
        "branch",
        "commit",
        "base_layer",
        "root",
        "id",
        "namespace",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "mount",
    &[a("sandbox", Name), a("branch", Name)],
    &[opt("name", Name)],
    "mount a Branch in a Sandbox",
    run,
)
.form(&FORM)
.scoped(Rule::WorkspaceMount)
.policy(Policy::Sandbox("sandbox"));

/// Runs `workspace.mount`: the handle `<sandbox>:<name>`, the name the
/// Branch's own made valid when none is given.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = MountRequest {
        sandbox: text("sandbox").unwrap_or_default(),
        branch: text("branch"),
        name: text("name"),
    };
    let (home, events) = context.parts();
    Ok(mounted(&mount(home, events, &request)?))
}
