//! The host Sandbox operations `sandbox::family` runs on the Sandbox's
//! machine, here or in its worker (`sandbox.host`): the Sandbox operations
//! (`call`). The Workspace operations and the Workspace status read are
//! `workspace::dispatch`, which passes every other operation here
//! (layerfs-project#106).
//! The 1.1 `runtime.*` and `backend.*` tables are gone (#536 step 2).
use crate::registry;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::{self, refused, Outcome};

/// Runs one Sandbox operation once; a Commit `message` is checked for every
/// operation.
pub fn call(op: &str, args: SdkValue) -> Outcome {
    checked(&args)?;
    match op {
        "sandbox.list_workspaces" => {
            let directory = crate::files::sandbox_dir(&args)?;
            listed(daemon_backend(op, args)?, &directory)
        }
        "sandbox.create" => crate::lifecycle::create(&args),
        "sandbox.close" => crate::lifecycle::close(&args),
        _ => Err(refused("UnknownMethod", format!("no operation {op}"))),
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
fn daemon_backend(op: &str, args: SdkValue) -> Outcome {
    let state = registry::sandbox_live(&args)?;
    if let Some(path) = args.get_text("path") {
        crate::unmount_guard::check(&state, path)?;
    }
    let args = match args {
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

/// Refuses an invalid Commit `message` argument before any effect.
pub fn checked(args: &SdkValue) -> Result<(), layerfs_bridge::control::SdkRefusal> {
    if let Some(value) = args.get("message") {
        if !matches!(value, SdkValue::Null) {
            let SdkValue::Text(message) = value else {
                return Err(refused("InvalidInput", "message must be text"));
            };
            let mut meta = layerfs_history::CommitMeta::new(layerfs_history::CommitVia::Files);
            meta.message = Some(message.clone());
            meta.check()
                .map_err(|error| refused("InvalidInput", error.to_string()))?;
        }
    }
    Ok(())
}
