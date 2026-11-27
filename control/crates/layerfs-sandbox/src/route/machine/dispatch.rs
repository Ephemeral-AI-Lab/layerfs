//! The host Sandbox operations `sandbox::family` runs on the Sandbox's
//! machine, here or in its worker (`sandbox.host`): the Sandbox operations
//! (`call`). The Workspace operations and the Workspace status read are
//! `workspace::dispatch`, which passes every other operation here
//! (layerfs-project#106). Both match on the one table of host operations
//! (`host::Call`, R1).
//! The 1.1 `runtime.*` and `backend.*` tables are gone (#536 step 2).
use crate::lifecycle::registry;
use crate::route::machine::{Call, Host, Message};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_protocol::sdk::SdkValue;

/// Runs one Sandbox operation once; a Commit `message` is checked for every
/// operation. A forced close unmounts each Workspace with `drain`.
pub fn call(
    host: Host,
    call: Call,
    drain: &dyn crate::lifecycle::serve::WorkspaceDrain,
) -> Outcome {
    checked(&host.message)?;
    match call {
        Call::ListWorkspaces { .. } => {
            let directory = crate::record::discovery::sandbox_dir(&host)?;
            listed(daemon_backend(&host, &call)?, &directory)
        }
        // the operator view of control admission and retained failures
        // (#540): a bounded read model, never a product operation
        Call::Capacity => capacity(&host),
        Call::Create { platform, place } => {
            crate::lifecycle::serve::create(&host, platform.as_deref(), place.as_ref())
        }
        Call::Close { force, held } => {
            crate::lifecycle::serve::close(&host, force, held.as_deref().unwrap_or_default(), drain)
        }
        _ => Err(refused(
            "UnknownMethod",
            format!("no operation {}", call.op().name()),
        )),
    }
}

/// [`call`] for the Sandbox's own calls that never close it (its start, its
/// reads and a cancel), as a [`crate::route::Local`]: a Close through it is
/// refused, since only the control-layer workspace crate's dispatch, which
/// drains the Sandbox's Workspaces, may run one.
pub fn unclosing(host: Host, call: Call) -> Outcome {
    if let Call::Close { .. } = call {
        return Err(refused(
            "UnknownMethod",
            "a Sandbox close runs through the Workspace dispatch",
        ));
    }
    self::call(host, call, &Unclosing)
}

/// The drain [`unclosing`] passes, which no Close ever reaches.
struct Unclosing;

impl crate::lifecycle::serve::WorkspaceDrain for Unclosing {
    fn drain(&self, _host: &Host, _path: &str) {}
}

/// `sandbox.list_workspaces`: the daemon's read model without its history
/// half, `{ kind, version, workspaces }`, which is all a daemon asked with
/// `history: false` answers; the SDK pages and shapes them.
/// Each Workspace gains its `path` (the daemon's `directory`), and the
/// `name` and `created` (when it was mounted) its record in Sandbox folder
/// `directory` gives it, or null; the
/// commands `running` in it, from the exec records of that folder (null
/// when they cannot be read); and its `overlay_bytes`, read by path where
/// this side has the Sandbox's run folder, else null (`crate::status::run_folder`;
/// layerfs-project#178); and its cgroup's `usage` with `usage_reason`,
/// read with them. None asks the daemon, takes a lock or writes.
fn listed(answer: SdkValue, directory: &std::path::Path) -> Outcome {
    let SdkValue::Map(fields) = answer else {
        return Err(refused(
            "Failed",
            "the daemon's Workspace list is not a record",
        ));
    };
    let records = crate::record::discovery::workspaces(directory)?;
    let record_at = |path: Option<&str>| {
        (records.iter())
            .find(|(_, r)| path.is_some() && r.path() == path)
            .map(|(_, r)| r)
    };
    let commands = crate::record::exec::running(directory);
    let running_in = |workspace: Option<&str>| match &commands {
        None => SdkValue::Null,
        Some(commands) => SdkValue::List(
            (commands.iter())
                .filter(|command| Some(command.workspace.as_str()) == workspace)
                .map(|command| command.shown())
                .collect(),
        ),
    };
    let named = |item: SdkValue| {
        let path = item.get("directory").cloned().unwrap_or_default();
        let record = record_at(item.get_text("directory"));
        let name = SdkValue::opt_text(record.and_then(|r| r.name()).map(str::to_owned));
        let created = (record.and_then(|r| r.created())).map_or(SdkValue::Null, SdkValue::Int);
        let running = running_in(item.get_text("workspace"));
        let item = layerfs_home::home::with_field(item, "path", path);
        let item = layerfs_home::home::with_field(item, "name", name);
        let item = layerfs_home::home::with_field(item, "created", created);
        layerfs_home::home::with_field(item, "running", running)
    };
    let listed = SdkValue::Map(
        fields
            .into_iter()
            .filter(|(name, _)| matches!(name.as_str(), "kind" | "version" | "workspaces"))
            .map(|(field, value)| match (field.as_str(), value) {
                ("workspaces", SdkValue::List(items)) => (
                    field,
                    SdkValue::List(items.into_iter().map(named).collect()),
                ),
                (_, value) => (field, value),
            })
            .collect(),
    );
    let read =
        crate::status::run_folder::read(directory, &crate::status::run_folder::namespaces(&listed));
    Ok(crate::status::run_folder::merged(listed, read.as_ref()))
}

/// A daemon backend call through the named Sandbox of the project: its
/// Workspaces and the read model that lists them. `deploy` only selects how this process dials
/// the daemon, so it is not sent on.
fn daemon_backend(host: &Host, call: &Call) -> Outcome {
    let state = registry::sandbox_live(host)?;
    if let Some(path) = call.path() {
        crate::record::unmount_guard::check(&state, path)?;
    }
    let op = call.op().name();
    // the daemon's own request (its protocol, `Request::Sdk`) is the host
    // request's one args map. `history: false` asks for its Workspaces
    // alone (layerfs-project#181): `listed` keeps nothing else, so the
    // daemon reads no Branch page, Layer or Commit for it. A daemon from
    // before the option ignores it and answers the whole model.
    let args = match host.to_args(call) {
        SdkValue::Map(fields) => SdkValue::Map(
            fields
                .into_iter()
                .filter(|(name, _)| !matches!(name.as_str(), "sandbox" | "deploy"))
                .chain([("history".to_owned(), SdkValue::Bool(false))])
                .collect(),
        ),
        other => other,
    };
    let answer =
        crate::route::daemon::channel::operator::call(&mut *state.controls.lend()?, op, args);
    answer.map_err(crate::route::daemon::refusal::sdk_error)
}

/// The daemon's `backend.daemon.connection_failures` read model (#540): the
/// connection the Workspace list lent from the Sandbox's pool, under one
/// bound for admission and the exchange (#543). Nothing starts: the caller
/// asks only a Sandbox this process still reaches.
fn capacity(host: &Host) -> Outcome {
    let state = registry::sandbox_live(host)?;
    crate::status::capacity::lent(&state.controls)
}

/// Refuses an invalid Commit `message` before any effect.
pub fn checked(message: &Message) -> Result<(), layerfs_protocol::sdk::SdkRefusal> {
    match message {
        Message::Absent | Message::Null => Ok(()),
        Message::NotText => Err(refused("InvalidInput", "message must be text")),
        Message::Text(message) => {
            let mut meta = layerfs_history::CommitMeta::new(layerfs_history::CommitVia::Files);
            meta.message = Some(message.clone());
            meta.check()
                .map_err(|error| refused("InvalidInput", error.to_string()))
        }
    }
}
