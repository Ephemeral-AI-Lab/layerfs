//! Native argument conversion and disjoint public/operator method tables.
use crate::job::{refused, Job, Outcome};
#[cfg(unix)]
use crate::job;
use crate::{convert, direct, project};
#[cfg(unix)]
use crate::{exec, output, registry, workspace};
use layerfs_bridge::control::SdkValue;
use napi::{bindgen_prelude::AsyncTask, JsUnknown};

/// Runs one SDK operation; the Promise resolves `{ value }` or `{ refused }`.
pub(crate) fn call(op: String, args: JsUnknown) -> napi::Result<AsyncTask<Job>> {
    let mut args = convert::from_js(args)?;
    let scope = crate::trace::take_scope(&mut args);
    Ok(AsyncTask::new(
        Job::new(move || dispatch(&op, args)).traced(scope),
    ))
}

fn dispatch(op: &str, args: SdkValue) -> Outcome {
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
    #[cfg(unix)]
    {
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
    #[cfg(windows)]
    {
        // Workspace and Sandbox operations need a daemon; the Windows-native
        // Store route serves project, fs and branch calls in-process.
        match op {
            "project.init" => project::init(&args),
            "workspace.mount"
            | "workspace.status"
            | "workspace.commit"
            | "workspace.unmount"
            | "workspace.exec"
            | "workspace.exec_release" => Err(refused(
                "Unsupported",
                "Workspace and Sandbox operations need a daemon; the Windows-native Store route serves project, fs and branch calls in-process",
            )),
            _ if op.starts_with("operator.") => Err(refused(
                "UnknownMethod",
                "operator methods require operatorCall",
            )),
            _ if op.contains('.') => direct::call(op, args, false),
            _ => Err(refused("UnknownMethod", format!("no operation {op}"))),
        }
    }
}

/// A daemon operator call through the named Sandbox of the project: its
/// Workspaces and the read model that lists them. `deploy` only selects how this process dials
#[cfg(unix)]
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
            layerfs_api::sandbox::values::refusal(&failure).unwrap_or_else(|| job::failed(failure))
        }
        layerfs_api::SdkError::Mount(failure) => job::failed(failure),
    })
}

/// Fills the caller's page with spooled exec output; see [`output::page`].
pub(crate) fn exec_output(
    args: JsUnknown,
    page: napi::bindgen_prelude::Buffer,
) -> napi::Result<AsyncTask<Job>> {
    #[cfg(windows)]
    {
        let _ = (&args, &page);
        return Ok(AsyncTask::new(Job::new(move || {
            Err(refused(
                "Unsupported",
                "exec output needs a daemon Workspace; the Windows-native Store route serves project, fs and branch calls in-process",
            ))
        })));
    }
    #[cfg(unix)]
    {
        let args = convert::from_js(args)?;
        Ok(AsyncTask::new(Job::new(move || output::page(&args, page))))
    }
}

/// Opens or releases a Store of this process.
pub(crate) fn store(
    args: JsUnknown,
    run: fn(&SdkValue) -> Outcome,
) -> napi::Result<AsyncTask<Job>> {
    let args = convert::from_js(args)?;
    Ok(AsyncTask::new(Job::new(move || run(&args))))
}

/// Runs one internal operator call through its separate method table.
pub(crate) fn operator_call(op: String, args: JsUnknown) -> napi::Result<AsyncTask<Job>> {
    #[cfg(windows)]
    {
        if op.starts_with("operator.process.")
            || matches!(
                op.as_str(),
                "operator.sandbox.list"
                    | "operator.inspect.snapshot"
                    | "operator.workspace.overlay"
                    | "operator.workspace.changes"
            )
        {
            return Ok(AsyncTask::new(Job::new(move || {
                Err(refused(
                    "Unsupported",
                    "this operator call needs a daemon; the Windows-native Store route serves Store calls in-process",
                ))
            })));
        }
    }
    let mut args = convert::from_js(args)?;
    let scope = crate::trace::take_scope(&mut args);
    Ok(AsyncTask::new(
        Job::new(move || {
            #[cfg(unix)]
            {
                match op.as_str() {
                    "operator.process.start" => exec::exec(&args),
                    "operator.process.list" => exec::processes(&args),
                    "operator.process.cancel" => exec::cancel(&args),
                    "operator.project.progress" => crate::project::progress(&args),
                    "operator.sandbox.list" => workspace::list(&args),
                    "operator.inspect.history"
                    | "operator.store.stats"
                    | "operator.store.commits"
                    | "operator.store.overwritten"
                    | "operator.files.stats"
                    | "operator.files.history" => direct::call(&op, args, true),
                    "operator.inspect.snapshot"
                    | "operator.workspace.overlay"
                    | "operator.workspace.changes" => daemon_operator(&op, args),
                    _ => Err(refused(
                        "UnknownMethod",
                        format!("no operator operation {op}"),
                    )),
                }
            }
            #[cfg(windows)]
            {
                match op.as_str() {
                    "operator.project.progress" => crate::project::progress(&args),
                    "operator.inspect.history"
                    | "operator.store.stats"
                    | "operator.store.commits"
                    | "operator.store.overwritten"
                    | "operator.files.stats"
                    | "operator.files.history" => direct::call(&op, args, true),
                    _ => Err(refused(
                        "UnknownMethod",
                        format!("no operator operation {op}"),
                    )),
                }
            }
        })
        .traced(scope),
    ))
}
