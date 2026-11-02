//! The two disjoint host method tables (#428), moved from layerfs-node's
//! dispatch.rs (#533 step 2): the public table (`call`: project, branch, fs,
//! sandbox, workspace) and the backend table (`backend`: `backend.store.*`,
//! `backend.daemon.*` and the runtime's `runtime.*` reads and actions).
use crate::common::outcome::{self, refused, Outcome};
use crate::host::{exec, output, registry, workspace};
use crate::project::init as project;
use crate::store::service as direct;
use layerfs_bridge::control::SdkValue;

/// Runs one public SDK operation once.
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
        "project.init" => project::init(&args),
        "project.list_sandboxes" => workspace::sandboxes(&args),
        "sandbox.list_workspaces" => listed(daemon_backend(op, args)?),
        "sandbox.create" => crate::host::sandbox::create(&args),
        "sandbox.close" => crate::host::sandbox::close(&args),
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
    if let Some(path) = args.get_text("path") {
        crate::host::unmount::check(&state, path)?;
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
    let answer = crate::operator::call(
        &mut *crate::common::trace::control(&state.controls)?,
        op,
        args,
    );
    answer.map_err(|error| match error {
        crate::SdkError::Refused(refusal) => refusal,
        crate::SdkError::Operation(failure) => {
            crate::sandbox::values::refusal(&failure).unwrap_or_else(|| outcome::failed(failure))
        }
        crate::SdkError::Mount(failure) => outcome::failed(failure),
    })
}

/// Runs one backend or runtime operation once on its separate table.
pub fn backend(op: String, args: SdkValue) -> Outcome {
    match op.as_str() {
        "backend.daemon.workspace.status" => workspace::status(&args),
        "backend.daemon.process.start" => exec::exec(&args),
        "backend.daemon.process.list" => exec::processes(&args),
        "backend.daemon.process.cancel" => exec::cancel(&args),
        "runtime.project.get_init" => crate::project::init::progress(&args),
        // #506 Store-first: Stores, their projects, remotes and transfers
        "runtime.store.create" => crate::store::identity::create(&args),
        "runtime.store.list_projects" => crate::store::identity::list_projects(&args),
        "runtime.store.add_project" => crate::store::identity::add_project(&args),
        "runtime.store.remove_project" => crate::store::identity::remove_project(&args),
        "runtime.store.forget" => crate::store::identity::forget(&args),
        "runtime.store.push" => crate::host::sync::push(&args),
        "runtime.store.publish" => crate::host::sync::publish(&args),
        "runtime.store.send" => crate::host::sync::send(&args),
        "runtime.store.pull" => crate::host::sync::pull(&args),
        "runtime.store.load" => crate::host::sync::load_store(&args),
        "runtime.store.unlink" => crate::host::sync::unlink(&args),
        "runtime.store.get_link" => crate::host::sync::last(&args),
        // #506 E1-E9: local sizes and heads, copy checks, listing,
        // providers, PostgreSQL status and live job progress
        "runtime.store.stat" => crate::host::local_stores::stat_stores(&args),
        "runtime.store.check" => crate::host::copies::check(&args),
        "runtime.remote.list_stores" => crate::host::copies::list_stores(&args),
        "runtime.remote.providers" => crate::host::copies::providers(&args),
        "runtime.postgresql.status" => crate::store::postgres::status(&args),
        "runtime.sync.progress" => crate::host::progress::get(&args),
        "runtime.remote.list" => crate::host::sync::list(&args),
        "runtime.remote.add" => crate::host::sync::add(&args),
        "runtime.remote.remove" => crate::host::sync::remove(&args),
        "runtime.sandbox.reclaim" => workspace::reclaim(&args),
        "runtime.sandbox.records" => workspace::all_records(&args),
        "backend.daemon.sandbox.get_log" => crate::host::sandbox::get_log(&args),
        "backend.store.project.get_history"
        | "backend.store.project.get_storage"
        | "backend.store.project.list_overwritten"
        | "backend.store.branch.list_commit_storage"
        | "backend.store.fs.get_storage"
        | "backend.store.fs.list_revisions" => direct::call(&op, args, true),
        "backend.daemon.workspace.get_storage" | "backend.daemon.workspace.list_changes" => {
            daemon_backend(&op, args)
        }
        _ => Err(refused(
            "UnknownMethod",
            format!("no backend operation {op}"),
        )),
    }
}
