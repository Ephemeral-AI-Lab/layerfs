//! The host Sandbox operations `sandbox::family` runs on the Sandbox's
//! machine, here or in its worker (`sandbox.host`): the Sandbox operations
//! (`call`). The Workspace operations and the Workspace status read are
//! `workspace::dispatch`, which passes every other operation here
//! (layerfs-project#106). Both match on the one table of host operations
//! (`host::Call`, R1).
//! The 1.1 `runtime.*` and `backend.*` tables are gone (#536 step 2).
use crate::host::{Call, Host, Message};
use crate::registry;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::{self, refused, Outcome};

/// Runs one Sandbox operation once; a Commit `message` is checked for every
/// operation.
pub fn call(host: Host, call: Call) -> Outcome {
    checked(&host.message)?;
    match call {
        Call::ListWorkspaces { .. } => {
            let directory = crate::files::sandbox_dir(&host)?;
            listed(daemon_backend(&host, &call)?, &directory)
        }
        // the operator view of control admission and retained failures
        // (#540): a bounded read model, never a product operation
        Call::Capacity => capacity(&host),
        Call::Create { platform } => crate::lifecycle::create(&host, platform.as_deref()),
        Call::Close { force, held } => {
            crate::lifecycle::close(&host, force, held.as_deref().unwrap_or_default())
        }
        Call::Destroy => crate::lifecycle::destroy(&host),
        _ => Err(refused(
            "UnknownMethod",
            format!("no operation {}", call.op().name()),
        )),
    }
}

/// `sandbox.list_workspaces`: the daemon's read model without its history
/// half, `{ kind, version, workspaces }`; the SDK pages and shapes them.
/// Each Workspace gains its `path` (the daemon's `directory`) and the
/// `name` its record in Sandbox folder `directory` gives it, or null.
fn listed(answer: SdkValue, directory: &std::path::Path) -> Outcome {
    let SdkValue::Map(fields) = answer else {
        return Err(refused(
            "Failed",
            "the daemon's Workspace list is not a record",
        ));
    };
    let records = crate::files::workspaces(directory)?;
    let name_at = |path: Option<&str>| {
        let found = records
            .iter()
            .find(|(_, r)| path.is_some() && r.path() == path);
        SdkValue::opt_text(found.and_then(|(_, r)| r.name()).map(str::to_owned))
    };
    let named = |item: SdkValue| {
        let path = item.get("directory").cloned().unwrap_or_default();
        let name = name_at(item.get_text("directory"));
        let item = layerfs_home::home::with_field(item, "path", path);
        layerfs_home::home::with_field(item, "name", name)
    };
    Ok(SdkValue::Map(
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
    ))
}

/// A daemon backend call through the named Sandbox of the project: its
/// Workspaces and the read model that lists them. `deploy` only selects how this process dials
/// the daemon, so it is not sent on.
fn daemon_backend(host: &Host, call: &Call) -> Outcome {
    let state = registry::sandbox_live(host)?;
    if let Some(path) = call.path() {
        crate::unmount_guard::check(&state, path)?;
    }
    let op = call.op().name();
    // the daemon's own request (its protocol, `Request::Sdk`) is the host
    // request's one args map
    let args = match host.to_args(call) {
        SdkValue::Map(fields) => SdkValue::Map(
            fields
                .into_iter()
                .filter(|(name, _)| !matches!(name.as_str(), "sandbox" | "deploy"))
                .collect(),
        ),
        other => other,
    };
    let answer = crate::daemon::operator::call(
        &mut *crate::daemon::trace::control(&state.controls)?,
        op,
        args,
    );
    answer.map_err(|error| match error {
        crate::SdkError::Refused(refusal) => refusal,
        crate::SdkError::Operation(failure) => {
            crate::daemon::values::refusal(&failure).unwrap_or_else(|| outcome::failed(failure))
        }
        crate::SdkError::Mount(failure) => outcome::failed(failure),
    })
}

/// The daemon's `backend.daemon.connection_failures` read model (#540): the
/// connection the Workspace list lent from the Sandbox's pool, under one
/// bound for admission and the exchange (#543). Nothing starts: the caller
/// asks only a Sandbox this process still reaches.
fn capacity(host: &Host) -> Outcome {
    let state = registry::sandbox_live(host)?;
    crate::daemon::capacity::lent(&state.controls)
}

/// Refuses an invalid Commit `message` before any effect.
pub fn checked(message: &Message) -> Result<(), layerfs_bridge::control::SdkRefusal> {
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
