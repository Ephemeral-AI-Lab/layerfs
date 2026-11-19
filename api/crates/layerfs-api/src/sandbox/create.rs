//! `sandbox.create`: start a named Sandbox of a project on a machine; with
//! `--mount`, also mount a Branch in it.
use super::get::facts;
use crate::calls::table::Kind::{Image, Name, Project};
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::{Context, Form, Policy};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::{public_code, with_fix, with_next, Outcome, FIX};
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

/// Runs `sandbox.create`. A start that fails on `native` with a daemon of
/// another build says so, with a fix that records this build's daemon
/// ([`layerfs_control::home::native_start`]). With no `machine` the Sandbox runs on its sqlite
/// Store's machine, else on `native`. With `mount`, the started Sandbox,
/// once recorded and ready, mounts that Branch under the mount's default
/// name (the Workspace package's `default_name`); a
/// mount that is refused keeps the Sandbox and answers the mount's refusal
/// with `sandbox` and a `fix` naming `layerfs workspace mount`: the
/// mount's own (an `Exists` names `--name`), else one that mounts again,
/// forking a Branch that is not there first. Progress events are
/// the image copy's phases, then `start` and, with `mount`, `mount`.
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
    // a native start that failed with a daemon of another build says so
    let created = create(home, events, &request).map_err(|refusal| {
        layerfs_control::home::native_start(home, crate::home_family::build(), refusal)
    })?;
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
        events.phase("mount", Some(1), Some(2), Vec::new());
        let asked = MountRequest {
            sandbox: request.name.clone(),
            branch: Some(branch.clone()),
            name: None,
        };
        let mounted = mount(home, events, &asked).map_err(|refusal| {
            let sandbox = &request.name;
            let again = format!("layerfs workspace mount {sandbox} {branch}");
            // the mount's own fix stays (an `Exists` names `--name`); a
            // Branch that is not there is forked first
            let fix = match public_code(&refusal.code) {
                _ if refusal.detail.get(FIX).is_some() => None,
                "NotFound" => Some(format!(
                    "layerfs branch fork {} {branch} && {again}",
                    request.project
                )),
                _ => Some(again),
            };
            let refusal = with_next(refusal, "sandbox", SdkValue::text(sandbox));
            match fix {
                Some(fix) => with_fix(refusal, fix),
                None => refusal,
            }
        })?;
        events.phase("mount", Some(2), Some(2), Vec::new());
        fields[0].1 = SdkValue::Text(mounted.ws);
        fields[1].1 = SdkValue::opt_text(mounted.answer.get_text("path"));
    }
    Ok(SdkValue::Map(
        (fields.into_iter())
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    ))
}
