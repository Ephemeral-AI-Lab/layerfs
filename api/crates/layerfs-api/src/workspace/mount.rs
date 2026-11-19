//! `workspace.mount`: mount a Branch in a Sandbox.
use crate::calls::scope::Rule;
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form, Policy};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::workspace::answers::mounted;
use layerfs_sandbox::workspace::calls::{mount, MountRequest};

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
