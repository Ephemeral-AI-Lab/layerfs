//! The host Sandbox operations `sandbox::family` runs on the Sandbox's
//! machine, here or in its worker (`sandbox.host`): the Sandbox and
//! Workspace operations (`call`) and the Workspace status read (`backend`).
//! The 1.1 `runtime.*` and `backend.*` tables are gone (#536 step 2).
use crate::common::outcome::{self, refused, Outcome};
use crate::sandbox::{exec, registry};
use crate::workspace::serve as workspace;
use layerfs_bridge::control::SdkValue;

/// Runs one Sandbox or Workspace operation once.
pub fn call(op: &str, args: SdkValue) -> Outcome {
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
    match op {
        "sandbox.list_workspaces" => listed(daemon_backend(op, args)?),
        "sandbox.create" => crate::sandbox::serve::create(&args),
        "sandbox.close" => crate::sandbox::serve::close(&args),
        "workspace.mount" => workspace::mount(&args),
        "workspace.commit" => workspace::commit(&args),
        "workspace.unmount" => workspace::unmount(&args),
        "workspace.exec" => exec::exec(&args),
        _ => Err(refused("UnknownMethod", format!("no operation {op}"))),
    }
}

/// `sandbox.list_workspaces`: the daemon's read model without its history
/// half, `{ kind, version, workspaces }`; the SDK pages and shapes them.
fn listed(answer: SdkValue) -> Outcome {
    let SdkValue::Map(fields) = answer else {
        return Err(refused(
            "Failed",
            "the daemon's Workspace list is not a record",
        ));
    };
    Ok(SdkValue::Map(
        fields
            .into_iter()
            .filter(|(name, _)| matches!(name.as_str(), "kind" | "version" | "workspaces"))
            .collect(),
    ))
}

/// A daemon backend call through the named Sandbox of the project: its
/// Workspaces and the read model that lists them. `deploy` only selects how this process dials
/// the daemon, so it is not sent on.
fn daemon_backend(op: &str, args: SdkValue) -> Outcome {
    let state = registry::sandbox_live(&args)?;
    if let Some(path) = args.get_text("path") {
        crate::workspace::unmount_guard::check(&state, path)?;
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
        &mut *crate::common::trace::control(&state.controls)?,
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

/// Runs the Workspace status read once.
pub fn backend(op: String, args: SdkValue) -> Outcome {
    match op.as_str() {
        "backend.daemon.workspace.status" => workspace::status(&args),
        _ => Err(refused(
            "UnknownMethod",
            format!("no backend operation {op}"),
        )),
    }
}
