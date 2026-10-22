//! Disjoint public/operator method tables over the session layer.
use crate::outcome::{failed, refused, Outcome};
use crate::{direct, exec, output, project, registry, workspace};
use layerfs_bridge::control::SdkValue;

/// Runs one public SDK operation.
pub fn dispatch(op: &str, args: SdkValue) -> Outcome {
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
        "project.init" => project::init(&args),
        "workspace.mount" => workspace::mount(&args),
        "workspace.status" => workspace::status(&args),
        "workspace.commit" => workspace::commit(&args),
        "workspace.unmount" => workspace::unmount(&args),
        "workspace.exec" if args.get_bool("background") => Err(refused(
            "InvalidInput",
            "background launch requires the operator process table",
        )),
        "workspace.exec" => exec::exec(&args),
        "workspace.exec_release" => output::release(&args),
        _ if op.starts_with("operator.") => Err(refused(
            "UnknownMethod",
            "operator methods require operatorCall",
        )),
        // Store calls run in this process; there is no other route.
        _ if op.contains('.') => direct::call(op, args, false),
        _ => Err(refused("UnknownMethod", format!("no operation {op}"))),
    }
}

/// Runs one internal operator operation through its separate method table.
pub fn operator_call(op: &str, args: SdkValue) -> Outcome {
    match op {
        "operator.process.start" => exec::exec(&args),
        "operator.process.list" => exec::processes(&args),
        "operator.process.cancel" => exec::cancel(&args),
        "operator.project.progress" => project::progress(&args),
        "operator.sandbox.list" => workspace::list(&args),
        "operator.inspect.history"
        | "operator.store.stats"
        | "operator.store.commits"
        | "operator.store.overwritten"
        | "operator.files.stats"
        | "operator.files.history" => direct::call(op, args, true),
        "operator.inspect.snapshot"
        | "operator.workspace.overlay"
        | "operator.workspace.changes" => daemon_operator(op, args),
        _ => Err(refused(
            "UnknownMethod",
            format!("no operator operation {op}"),
        )),
    }
}

/// A daemon operator call through the named Sandbox of the project: its
/// Workspaces and the read model that lists them. `deploy` only selects how this process dials
/// the daemon, so it is not sent on.
fn daemon_operator(op: &str, args: SdkValue) -> Outcome {
    let state = registry::sandbox_live(&args)?;
    let args = match args {
        SdkValue::Map(fields) => SdkValue::Map(
            fields
                .into_iter()
                .filter(|(name, _)| !matches!(name.as_str(), "sandbox" | "deploy"))
                .collect(),
        ),
        other => other,
    };
    let answer =
        layerfs_api::operator::call(&mut *crate::trace::control(&state.controls)?, op, args);
    answer.map_err(|error| match error {
        layerfs_api::SdkError::Refused(refusal) => refusal,
        layerfs_api::SdkError::Operation(failure) => {
            layerfs_api::sandbox::values::refusal(&failure).unwrap_or_else(|| failed(failure))
        }
        layerfs_api::SdkError::Mount(failure) => failed(failure),
    })
}

/// Fills `page` with spooled exec output; see [`output::page`].
pub fn exec_output(args: &SdkValue, page: &mut [u8]) -> Outcome {
    output::page(args, page)
}
