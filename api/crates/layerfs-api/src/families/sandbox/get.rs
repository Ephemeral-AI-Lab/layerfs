//! `sandbox.get`: one Sandbox, its state and its Workspaces, a live read of
//! its daemon; with `--with stats`, its daemon's process and capacity and
//! what it and each Workspace use. This file also holds the helpers the
//! family's entries share to answer what a library function returned under
//! the rows' names.
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{a, call, Call};
use crate::dispatch::{Context, Form, Part, Section};
use layerfs_home::reply::outcome::Outcome;
use layerfs_home::reply::time::iso;
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::family::{get, GetRequest, Got, ListedWorkspace};
use layerfs_sandbox::record::SandboxRecord;
use layerfs_sandbox::status::state::activity;
use layerfs_sandbox::status::stats::shown_usage;

/// One command running in a Workspace.
const COMMAND: Form = Form::of(&["exec"], &[], &["exec", "started_at", "program", "tag"]);
/// One Workspace: its handle and Branch, the commands running in it and
/// its state when not ready, then what its daemon and record say of it.
const WORKSPACE: Form = Form::of(
    &["workspace", "branch"],
    &["running", "state", "op", "reason", "fix"],
    &[
        "workspace",
        "branch",
        "running",
        "state",
        "op",
        "reason",
        "fix",
        "path",
        "commit",
        "base_layer",
        "mounted_at",
        "branch_moved",
        "pending_merge",
        "commands",
        "id",
    ],
)
.parts(&[
    ("running", Part::Scalars("exec")),
    ("commands", Part::Rows(&COMMAND)),
]);
const FORM: Form = Form::of(
    &["state"],
    &[
        "workspaces",
        "op",
        "blocked_by",
        "reason",
        "fix",
        "observed_at",
    ],
    &[
        "state",
        "workspaces",
        "op",
        "blocked_by",
        "reason",
        "fix",
        "observed_at",
        "message",
        "sandbox",
        "project",
        "machine",
        "platform",
        "image",
        "backend",
        "route",
        "created_at",
        "workspace_capacity",
        "network",
    ],
)
.parts(&[("workspaces", Part::Rows(&WORKSPACE))]);

/// What one Workspace uses, in `--with stats`.
const STATS_WORKSPACE: Form = Form::of(
    &["workspace"],
    &["overlay_bytes", "usage", "usage_reason"],
    &["workspace", "overlay_bytes", "usage", "usage_reason"],
);
/// The keys of `--with stats`.
const STATS_KEYS: [&str; 5] = [
    "daemon",
    "usage",
    "usage_reason",
    "daemon_capacity",
    "workspaces",
];
const STATS: Form =
    Form::of(&[], &STATS_KEYS, &STATS_KEYS).parts(&[("workspaces", Part::Rows(&STATS_WORKSPACE))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "get",
    &[a("sandbox", Name)],
    &[],
    "one Sandbox: its state and its Workspaces",
    run,
)
.form(&FORM)
.with(&[Section::own("stats", &STATS_KEYS).shaped(&STATS)]);

/// Runs `sandbox.get`. Its state starts from records; a Sandbox they say
/// is ready (or lost, or not answering) is asked for its Workspaces once,
/// on its usual route, and the answer decides: ready, `broken` `lost` or
/// `not_answering`, or `unknown` when its machine does not answer. The
/// record is written only when that changes why it is broken. `--with
/// stats` reads the daemon's capacity model, one more bounded call, only
/// for a ready Sandbox; the section is null for any other.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = GetRequest {
        name: args.get_text("sandbox").unwrap_or("").to_owned(),
        stats: context.with("stats"),
    };
    let (home, events) = context.parts();
    let got = get(home, events, &request)?;
    let workspaces = (got.workspaces.as_ref())
        .map(|listed| SdkValue::List(listed.iter().map(workspace).collect()));
    let mut fields = vec![
        ("workspaces", workspaces.unwrap_or_default()),
        ("message", SdkValue::opt_text(got.message.clone())),
        (
            "workspace_capacity",
            SdkValue::Int(i64::from(got.workspace_capacity)),
        ),
    ];
    fields.extend(facts(&got.record));
    fields.push(("network", SdkValue::opt_text(got.record.network())));
    if request.stats {
        fields.push(("stats", stats(&got)));
    }
    let answer = SdkValue::Map(
        (fields.into_iter())
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    );
    Ok(got.state.write(answer))
}

/// What a Sandbox's record holds, as the rows name it.
pub(super) fn facts(record: &SandboxRecord) -> Vec<(&'static str, SdkValue)> {
    vec![
        ("sandbox", SdkValue::text(record.name())),
        ("project", SdkValue::opt_text(record.project())),
        ("machine", SdkValue::text(record.machine())),
        ("platform", SdkValue::opt_text(record.platform())),
        ("image", SdkValue::opt_text(record.image())),
        ("backend", SdkValue::opt_text(record.backend())),
        ("route", SdkValue::opt_text(record.route())),
        ("created_at", at(record.created_ms())),
    ]
}

/// A time in milliseconds since 1970 as answers spell it (`..._at`).
pub(super) fn at(ms: Option<i64>) -> SdkValue {
    ms.map_or(SdkValue::Null, |ms| SdkValue::Text(iso(ms)))
}

/// `value`'s field `name`, null when it has none.
pub(super) fn field(value: &SdkValue, name: &str) -> SdkValue {
    value.get(name).cloned().unwrap_or_default()
}

/// One Workspace its daemon listed, as the row names it: its state from
/// its daemon's `activity`.
fn workspace(listed: &ListedWorkspace) -> SdkValue {
    let item = &listed.item;
    let commands: Vec<&SdkValue> = match item.get("running") {
        Some(SdkValue::List(commands)) => commands.iter().collect(),
        _ => Vec::new(),
    };
    let running = (commands.iter()).map(|command| field(command, "exec_id"));
    let shown = (commands.iter()).map(|command| {
        SdkValue::map([
            ("exec", field(command, "exec_id")),
            ("started_at", at(command.get_int("started"))),
            ("program", field(command, "program")),
            ("tag", field(command, "tag")),
        ])
    });
    let state = activity(item.get_text("activity").unwrap_or("idle"), &listed.ws);
    state.write(SdkValue::map([
        ("workspace", SdkValue::text(&listed.ws)),
        ("branch", field(item, "branch")),
        ("running", SdkValue::List(running.collect())),
        ("path", SdkValue::text(&listed.path)),
        ("commit", field(item, "base")),
        // as its daemon spells them
        ("base_layer", field(item, "baseLayer")),
        ("mounted_at", at(item.get_int("created"))),
        ("branch_moved", field(item, "headMovedSinceMount")),
        ("pending_merge", field(item, "pendingMerge")),
        ("commands", SdkValue::List(shown.collect())),
        ("id", field(item, "workspace")),
    ]))
}

/// `--with stats`: null unless the Sandbox is ready.
fn stats(got: &Got) -> SdkValue {
    if got.state != layerfs_home::reply::state::State::Ready {
        return SdkValue::Null;
    }
    let process = |name: &str| {
        (got.process.iter())
            .find(|(held, _)| held == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let daemon = match process("daemon") {
        daemon @ SdkValue::Map(_) => SdkValue::map([
            ("pid", field(&daemon, "pid")),
            ("started_at", at(daemon.get_int("started_at"))),
        ]),
        _ => SdkValue::Null,
    };
    let workspaces = (got.workspaces.iter().flatten()).map(|listed| {
        SdkValue::map([
            ("workspace", SdkValue::text(&listed.ws)),
            ("overlay_bytes", field(&listed.item, "overlay_bytes")),
            ("usage", shown_usage(&field(&listed.item, "usage"))),
            ("usage_reason", field(&listed.item, "usage_reason")),
        ])
    });
    let capacity = (got.daemon_capacity.as_ref()).map(|model| model.to_sdk());
    SdkValue::map([
        ("daemon", daemon),
        ("usage", shown_usage(&process("usage"))),
        ("usage_reason", process("usage_reason")),
        ("daemon_capacity", capacity.unwrap_or_default()),
        ("workspaces", SdkValue::List(workspaces.collect())),
    ])
}
