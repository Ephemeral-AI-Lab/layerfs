//! `sandbox.create`: start a named Sandbox of a project on a machine; with
//! `--mount`, also mount a Branch in it.
use super::get::facts;
use crate::calls::table::Kind::{Image, Name, Project};
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::{Context, Form, Policy};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::{with_fix, with_next, Outcome};
use layerfs_sandbox::family::{create, CreateRequest};
use layerfs_sandbox::workspace::calls::{mount, MountRequest};

/// Its answer: `ok`, or with `--mount` the Workspace's handle and path.
const FORM: Form = Form::of(
    &[],
    &["ok", "workspace", "path"],
    &[
        "ok",
        "workspace",
        "path",
        "outcome",
        "sandbox",
        "project",
        "machine",
        "platform",
        "image",
        "backend",
        "route",
        "state",
        "created_at",
        "forward_failed",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "create",
    &[a("project", Project), a("name", Name)],
    &[
        opt("machine", Name),
        opt("image", Image),
        opt("mount", Name),
        PROGRESS,
    ],
    "start a named Sandbox of a project on a machine; with --mount, also mount a Branch in it",
    run,
)
.form(&FORM)
.agent()
.policy(Policy::NewSandbox);

/// A Workspace's default name: the Branch's name made valid (review fix
/// 12), lowercased with each character outside the name alphabet (`a-z`,
/// `0-9`, `_`, `.`, `-`) replaced by `-`. A name still invalid (one that
/// starts with `-`, say) is refused by the mount.
fn default_name(branch: &str) -> String {
    let valid = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || "_.-".contains(c);
    let lower = branch.to_lowercase();
    lower
        .chars()
        .map(|c| if valid(c) { c } else { '-' })
        .collect()
}

/// Runs `sandbox.create`. With no `machine` the Sandbox runs on its sqlite
/// Store's machine, else on `native`. With `mount`, the started Sandbox,
/// once recorded and ready, mounts that Branch under its default name; a
/// mount that is refused keeps the Sandbox and answers the mount's refusal
/// with `sandbox` and a `fix` that mounts it again. Progress events are
/// its phases: `claim`, `start` and, with `mount`, `mount`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let branch = text("mount");
    let request = CreateRequest {
        project: text("project").unwrap_or_default(),
        name: text("name").unwrap_or_default(),
        machine: text("machine"),
        image: text("image"),
        mount: branch.is_some(),
    };
    let (home, events) = context.parts();
    let created = create(home, events, &request)?;
    let mut fields = vec![
        ("workspace", SdkValue::Null),
        ("path", SdkValue::Null),
        ("outcome", SdkValue::text("created")),
        ("state", SdkValue::text("ready")),
        (
            "forward_failed",
            SdkValue::opt_text(created.forward_failed.clone()),
        ),
    ];
    fields.extend(facts(&created.record));
    if let Some(branch) = branch {
        events.phase("mount", Some(2), Some(3), Vec::new());
        let asked = MountRequest {
            sandbox: request.name.clone(),
            branch: Some(branch.clone()),
            name: Some(default_name(&branch)),
        };
        let mounted = mount(home, events, &asked).map_err(|refusal| {
            let fix = format!("layerfs workspace mount {} {branch}", request.name);
            let refusal = with_next(refusal, "sandbox", SdkValue::text(&request.name));
            with_fix(refusal, fix)
        })?;
        events.phase("mount", Some(3), Some(3), Vec::new());
        fields[0].1 = SdkValue::Text(mounted.ws);
        fields[1].1 = SdkValue::opt_text(mounted.answer.get_text("path"));
    }
    Ok(SdkValue::Map(
        (fields.into_iter())
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    ))
}
