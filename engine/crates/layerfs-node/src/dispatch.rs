//! Native argument conversion and the two disjoint method tables (#428): the
//! public table (`call`: project, branch, fs, sandbox, workspace) and the
//! backend table (`backendCall`: `backend.store.*`, `backend.daemon.*` and the
//! runtime's `runtime.*` reads and actions).
use crate::job::{refused, Job, Outcome};
use crate::{convert, direct, exec, job, output, project, registry, workspace};
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
    crate::views::guard(op, &args)?;
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
        "project.list_sandboxes" => workspace::sandboxes(&args),
        "sandbox.list_workspaces" => listed(daemon_backend(op, args)?),
        "sandbox.create" => crate::sandbox::create(&args),
        "sandbox.close" => crate::sandbox::close(&args),
        "workspace.mount" => workspace::mount(&args),
        "workspace.commit" => workspace::commit(&args),
        "workspace.unmount" => workspace::unmount(&args),
        "workspace.exec" if args.get_bool("background") => Err(refused(
            "InvalidInput",
            "background launch is backend.daemon.process.start",
        )),
        "workspace.exec" => exec::exec(&args),
        "workspace.exec_release" => output::release(&args),
        _ if op.starts_with("backend.") || op.starts_with("runtime.") => Err(refused(
            "UnknownMethod",
            "backend and runtime methods require backendCall",
        )),
        // Store calls run in this process; there is no other route.
        _ if op.contains('.') => direct::call(op, args, false),
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
    let args = convert::from_js(args)?;
    Ok(AsyncTask::new(Job::new(move || output::page(&args, page))))
}

/// Opens or releases a Store of this process.
pub(crate) fn store(
    args: JsUnknown,
    run: fn(&SdkValue) -> Outcome,
) -> napi::Result<AsyncTask<Job>> {
    let args = convert::from_js(args)?;
    Ok(AsyncTask::new(Job::new(move || run(&args))))
}

/// Runs one backend or runtime call through its separate method table.
pub(crate) fn backend_call(op: String, args: JsUnknown) -> napi::Result<AsyncTask<Job>> {
    let mut args = convert::from_js(args)?;
    let scope = crate::trace::take_scope(&mut args);
    Ok(AsyncTask::new(
        Job::new(move || backend(&op, args)).traced(scope),
    ))
}

/// One backend or runtime call; a view's store path takes reads only (#510).
fn backend(op: &str, args: SdkValue) -> Outcome {
    crate::views::guard(op, &args)?;
    match op {
        "backend.daemon.workspace.status" => workspace::status(&args),
        "backend.daemon.process.start" => exec::exec(&args),
        "backend.daemon.process.list" => exec::processes(&args),
        "backend.daemon.process.cancel" => exec::cancel(&args),
        "runtime.project.get_init" => crate::project::progress(&args),
        // #506 Store-first: Stores, their projects, remotes and transfers
        "runtime.store.create" => crate::stores::create(&args),
        "runtime.store.list_projects" => crate::stores::list_projects(&args),
        "runtime.store.add_project" => crate::stores::add_project(&args),
        "runtime.store.remove_project" => crate::stores::remove_project(&args),
        "runtime.store.forget" => crate::stores::forget(&args),
        "runtime.store.push" => crate::sync::push(&args),
        "runtime.store.publish" => crate::sync::publish(&args),
        "runtime.store.send" => crate::sync::send(&args),
        "runtime.store.pull" => crate::sync::pull(&args),
        "runtime.store.load" => crate::sync::load_store(&args),
        "runtime.store.unlink" => crate::sync::unlink(&args),
        "runtime.store.get_link" => crate::sync::last(&args),
        "runtime.store.import_postgresql" => crate::postgres::import(&args),
        "runtime.store.attach_postgresql" => crate::postgres::attach(&args),
        // #506 E1-E9: local sizes and heads, copy checks, listing,
        // providers, PostgreSQL status and live job progress
        "runtime.store.stat" => crate::local::stat_stores(&args),
        "runtime.store.check" => crate::copies::check(&args),
        "runtime.remote.list_stores" => crate::copies::list_stores(&args),
        "runtime.remote.providers" => crate::copies::providers(&args),
        "runtime.postgresql.status" => crate::postgres::status(&args),
        "runtime.sync.progress" => crate::progress::get(&args),
        "runtime.remote.list" => crate::sync::list(&args),
        "runtime.remote.add" => crate::sync::add(&args),
        "runtime.remote.remove" => crate::sync::remove(&args),
        // #510: live read-only views of a remote Store file
        "runtime.remote.open_view" => crate::views::open(&args),
        "runtime.remote.close_view" => crate::views::close(&args),
        "runtime.remote.view_stats" => crate::views::stats(&args),
        "runtime.sandbox.reclaim" => workspace::reclaim(&args),
        "backend.daemon.sandbox.get_log" => crate::sandbox::get_log(&args),
        "backend.store.project.get_history"
        | "backend.store.project.get_storage"
        | "backend.store.project.list_overwritten"
        | "backend.store.branch.list_commit_storage"
        | "backend.store.fs.get_storage"
        | "backend.store.fs.list_revisions" => direct::call(op, args, true),
        "backend.daemon.workspace.get_storage" | "backend.daemon.workspace.list_changes" => {
            daemon_backend(op, args)
        }
        _ => Err(refused(
            "UnknownMethod",
            format!("no backend operation {op}"),
        )),
    }
}
