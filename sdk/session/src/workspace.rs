//! Workspace calls (#373). `workspace.mount(branch)` places the new
//! Workspace: under the project's guard it takes the lowest-numbered running
//! Sandbox of the call's image (`deploy.image`) with a free slot, reclaiming
//! any whose container is gone, and starts the next Sandbox, on that image,
//! when every such one is full. Sandboxes of one project may run different
//! images. A Workspace is
//! `{ sandbox, path }`; status, Commit and unmount go to its Sandbox's
//! daemon. Nothing is retried: a refused or failed step is the call's answer.
//! A Sandbox whose daemon ended itself when idle is found stopped by the
//! next mount or listing, which reclaims it.
use crate::outcome::{failed, Outcome};
use crate::registry::{
    lock, naming, random, sandbox, sandbox_live, sandbox_refreshed, text, Mount, SandboxState,
};
use crate::sandbox::{operation, WORKSPACES};
use layerfs_api::internal::workspace;
use layerfs_api::sandbox::values;
use layerfs_bridge::control::{CommitControl, SdkRefusal, SdkValue};
use layerfs_history::WorkspaceId;
use std::path::Path;
use std::sync::Arc;

/// The Workspaces Sandbox directory `directory` records.
fn recorded(directory: &Path) -> Result<usize, SdkRefusal> {
    let mut count = 0;
    for item in std::fs::read_dir(directory).map_err(failed)? {
        let name = item.map_err(failed)?.file_name();
        let name = name.to_string_lossy();
        count += usize::from(name.starts_with("workspace-") && name.ends_with(".record"));
    }
    Ok(count)
}

/// The Sandbox for a new Workspace, under the project's guard: the
/// lowest-numbered running one of the call's image with a free slot, else a
/// new one.
fn place(args: &SdkValue, project: &Path) -> Result<Arc<SandboxState>, SdkRefusal> {
    let image = args
        .get("deploy")
        .and_then(|deploy| deploy.get_text("image"));
    for id in crate::discovery::sandbox_ids(args)? {
        let directory = crate::discovery::sandbox_dir(args, &id)?;
        let record = directory.join("sandbox.record");
        // a number claimed by a start that has not finished is not a Sandbox yet
        if !record.exists() || recorded(&directory)? >= WORKSPACES as usize {
            continue;
        }
        if image.is_some() && crate::discovery::load(&record)?.get_text("image") != image {
            continue;
        }
        match sandbox_live(&naming(args, &id)) {
            Ok(state) => return Ok(state),
            Err(refusal) => match crate::discovery::gone_container(&refusal) {
                Some((container, stopped)) => {
                    crate::sandbox::reclaim(args, &id, container, stopped)?;
                }
                None => return Err(refusal),
            },
        }
    }
    crate::sandbox::start(args, project)
}

/// A daemon mount failure keeps its code and data, plus the acknowledged
/// binding when the first step completed.
fn mount_refusal(failure: Box<layerfs_api::MountFailure>) -> SdkRefusal {
    let layerfs_api::MountFailure { bound, failure } = *failure;
    let mut refusal = operation(failure);
    if let Some(bound) = bound {
        let mut detail = match refusal.detail {
            SdkValue::Map(fields) => fields,
            other => vec![("cause".to_owned(), other)],
        };
        detail.push((
            "workspace".to_owned(),
            SdkValue::Text(layerfs_bridge::sdk_values::hex(
                &bound.token.workspace.to_bytes(),
            )),
        ));
        detail.push(("namespace".to_owned(), SdkValue::Int(bound.token.namespace)));
        refusal.detail = SdkValue::Map(detail);
    }
    refusal
}

/// `workspace.mount(branch)`: a new Workspace of `branch`, placed in one of
/// the project's Sandboxes; its `sandbox` and `path`.
pub(crate) fn mount(args: &SdkValue) -> Outcome {
    let branch = text(args, "branch")?;
    let project = crate::discovery::project_dir(args)?;
    let _placing = crate::discovery::guard(&project)?;
    // The Branch is found by name in this process before placement, so a bad
    // Branch neither starts nor reclaims a Sandbox.
    let branch_id = crate::direct::branch_id(args, branch)?;
    let state = place(args, &project)?;
    let _guard = crate::discovery::guard(&state.directory)?;
    let id = WorkspaceId::from_authority(random()?).map_err(failed)?;
    let mounted = {
        let mut control = crate::trace::control(&state.controls)?;
        workspace(&mut control).mount(id, branch_id)
    };
    let mounted = mounted.map_err(mount_refusal)?;
    let mut value = values::mounted(&mounted, branch);
    if let SdkValue::Map(fields) = &mut value {
        fields.push(("sandbox".to_owned(), SdkValue::text(state.id.as_str())));
        fields.push(("container".to_owned(), SdkValue::text(state.name.as_str())));
        let image = state.runtime.request().image.as_str();
        fields.push(("image".to_owned(), SdkValue::text(image)));
    }
    crate::discovery::save_workspace(&state, &mounted, branch)?;
    lock(&state.mounts)?.insert(
        mounted.ready.directory.clone(),
        Mount {
            workspace: mounted,
            branch: branch.to_owned(),
        },
    );
    Ok(value)
}

/// `operator.sandbox.list()`: the project's running Sandboxes, lowest
/// number first: `{ id, name, image }`. Each recorded container is inspected once
/// (no daemon is dialed); one found gone or stopped, an idle-closed
/// Sandbox's, is reclaimed under the project's guard and left out. It starts
/// nothing.
pub(crate) fn list(args: &SdkValue) -> Outcome {
    let mut items = Vec::new();
    for id in crate::discovery::sandbox_ids(args)? {
        let directory = crate::discovery::sandbox_dir(args, &id)?;
        let record = directory.join("sandbox.record");
        if !record.exists() {
            continue;
        }
        let value = crate::discovery::load(&record)?;
        let item = SdkValue::map([
            ("id", SdkValue::text(&id)),
            (
                "name",
                SdkValue::text(value.get_text("name").unwrap_or_default()),
            ),
            (
                "image",
                SdkValue::text(value.get_text("image").unwrap_or_default()),
            ),
        ]);
        let cached = lock(&crate::registry::SANDBOXES)?.get(&directory).cloned();
        let observed = match cached {
            Some(state) => state.runtime.endpoint().map(drop),
            None => {
                let (docker, selector, request) = crate::discovery::runtime(&value)?;
                layerfs_sandbox::backend::docker::Sandbox::reopen(docker, selector, request)
                    .map(drop)
            }
        };
        match observed {
            Ok(()) => items.push(item),
            Err(failure) if failure.container_gone() || failure.container_stopped() => {
                let _placing = crate::discovery::guard(&crate::discovery::project_dir(args)?)?;
                let container = text(&value, "container")?;
                match crate::sandbox::reclaim(args, &id, container, failure.container_stopped()) {
                    Ok(_) => {}
                    // started again under the number meanwhile
                    Err(refusal) if refusal.code == "Busy" => items.push(item),
                    Err(refusal) => return Err(refusal),
                }
            }
            Err(failure) => return Err(failed(failure)),
        }
    }
    Ok(SdkValue::map([
        ("kind", SdkValue::text("ok")),
        ("items", SdkValue::List(items)),
    ]))
}

/// `workspace.status(ws)`: the daemon's Status, which reads no
/// Store, and the Branch head from one newest-first History read in this
/// process.
pub(crate) fn status(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let token = state.token(text(args, "path")?)?;
    let mut control = crate::trace::control(&state.controls)?;
    let status = workspace(&mut control).status(token).map_err(operation)?;
    drop(control);
    let head = crate::direct::head(args, status.binding.branch.id).ok();
    let mut value = values::status(&status, head);
    if let SdkValue::Map(fields) = &mut value {
        let branch = lock(&state.mounts)?
            .get(text(args, "path")?)
            .map(|mount| mount.branch.clone());
        fields.push(("branch".to_owned(), SdkValue::opt_text(branch)));
    }
    Ok(value)
}

/// `workspace.commit(ws, { message?, force? })`.
pub(crate) fn commit(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let token = state.token(text(args, "path")?)?;
    let reply = workspace(&mut *crate::trace::control(&state.controls)?)
        .commit_with(CommitControl {
            token,
            force: args.get_bool("force"),
            message: args.get_text("message").map(str::to_owned),
        })
        .map_err(operation)?;
    values::commit(&reply)
}

/// `workspace.unmount(ws, { discard? })`: `Busy` while a background
/// process this binding started still runs there, then the daemon's answer.
pub(crate) fn unmount(args: &SdkValue) -> Outcome {
    let state = sandbox_refreshed(args)?;
    let _guard = crate::discovery::guard(&state.directory)?;
    let path = text(args, "path")?;
    let token = state.token(path)?;
    // The runtime owner knows its own processes; the daemon's kernel probe
    // still answers Busy for any other holder.
    let running = crate::exec::running_in(&state, path)?;
    if !running.is_empty() {
        return Err(SdkRefusal {
            code: "Busy".to_owned(),
            message: format!(
                "{}:{path} is busy: {} still running",
                state.id,
                running.join(", ")
            ),
            detail: SdkValue::map([(
                "processes",
                SdkValue::List(running.into_iter().map(SdkValue::Text).collect()),
            )]),
        });
    }
    let answer = workspace(&mut *crate::trace::control(&state.controls)?)
        .unmount_checked(token, args.get_bool("discard"))
        .map_err(operation)?;
    let value = values::unmounted(&answer)?;
    std::fs::remove_file(crate::discovery::workspace_file(&state, token)).map_err(failed)?;
    lock(&state.mounts)?.remove(path);
    Ok(value)
}
