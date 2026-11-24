//! The forced close of a `native` Sandbox, which bypasses Store and daemon
//! discovery and live unmount admission: only its runtime stops; the
//! machine, its Stores and saved history stay. A forced `machine.stop` on
//! `native` stops each Sandbox's runtime the same way ([`native_host`],
//! `Recorded::stop_now`).
use crate::host::{Deploy, Host, Message};
use crate::mount_intents::{Bound, Branch};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused, Outcome};
use std::path::Path;

/// The host operation's view of `native` Sandbox `sandbox` from its own
/// records alone: no Store, project or daemon is resolved.
pub fn native_host(home: &Path, sandbox: &str) -> Result<Host, SdkRefusal> {
    let id = home::opened(home)?.id;
    Ok(Host {
        home: home.to_path_buf(),
        store: String::new(),
        project: String::new(),
        sandbox: Some(sandbox.to_owned()),
        store_record: None,
        store_path: None,
        deploy: Some(Deploy {
            local_dir: Some(paths::run_root(home).display().to_string()),
            container_prefix: Some(layerfs_home::names::provider_prefix(&id)),
            ..Deploy::default()
        }),
        postgresql: None,
        message: Message::Absent,
        started: None,
        reach_hint: None,
    })
}

/// The refusal detail field a forced close sets once its runtime stopped.
pub const RUNTIME_STOPPED: &str = "runtime_stopped";

/// Refuses while a live process holds `native` (a forced `machine.stop` in
/// progress, for one). An unresolved intent does not stop a forced close,
/// whether an ended process left it or this process did.
pub fn native_free(home: &Path) -> Result<(), SdkRefusal> {
    let Some(machine) = layerfs_machine::record::MachineRecord::read(home, "native")? else {
        return Ok(());
    };
    match machine.op() {
        Some(op) if op.mine() && op.uncertain() => Ok(()),
        _ => machine.not_running(),
    }
}

/// Forced `sandbox.remove` of the Sandbox `host` names, on this process's
/// kernel: no daemon control call, Store open, active-Workspace refusal or
/// retry. Its running commands are cancelled first, as every forced close
/// does (`crate::exec::cancel::settle`). The backend's Stop checks the runtime identity and ends its
/// mounts and processes; its Delete independently checks the physical
/// detach before the runtime goes. Answers each recorded or `held`
/// Workspace as `unknown`: its uncommitted changes end with the Sandbox.
/// Once the runtime is stopped, each Workspace's mount intent is ended
/// once (`crate::mount_intents`); one it could not end is in `left`.
/// A record this build does not read never reaches it: `sandbox.remove`
/// forgets it first (`crate::reopen::unread`).
pub fn close(host: &Host, held: &[String]) -> Outcome {
    let id = host.sandbox()?.to_owned();
    let directory = crate::files::sandbox_dir(host)?;
    let _guard = layerfs_store::project::discovery::guard(&directory)?;
    let value = crate::files::started(&directory)?
        .ok_or_else(|| refused("NoSuchSandbox", format!("no Sandbox {id} is recorded")))?;
    let mut bound = crate::files::workspaces(&directory)?
        .into_iter()
        .map(|(path, record)| Bound {
            path: (record.path().map(str::to_owned)).unwrap_or_else(|| path.display().to_string()),
            bound: recorded_bound(&record),
        })
        .collect::<Vec<_>>();
    for path in held {
        if !bound.iter().any(|known| &known.path == path) {
            let path = path.to_owned();
            bound.push(Bound { path, bound: None });
        }
    }
    // its running commands are each marked and cancelled first (D-10), so
    // they answer `cancelled`, not the exit of the runtime's kill
    let unreached = &mut crate::exec::cancel::unreached;
    crate::exec::cancel::settle(&directory, &id, None, true, unreached)?;
    crate::reopen::recorded(host, &id, &value)?.stop_now()?;
    // The runtime is confirmed stopped from here on: a later failure says so,
    // so the record shows it stopped instead of as it was.
    let after_stop = |mut refusal: SdkRefusal| {
        refusal.detail = home::with_field(refusal.detail, RUNTIME_STOPPED, SdkValue::Bool(true));
        refusal
    };
    crate::reopen::recorded(host, &id, &value)
        .map_err(after_stop)?
        .stopped()
        .delete()
        .map_err(|error| after_stop(crate::daemon::values::wire(error)))?;
    // their owner is stopped: each Workspace's mount intent is ended once
    let record = crate::record::SandboxRecord::read(&host.home, &id).map_err(after_stop)?;
    let open = match &record {
        Some(record) => crate::mount_intents::store_of(&host.home, record),
        None => Err(format!("no Sandbox {id} is recorded")),
    };
    // the runtime is confirmed stopped: write the durable home tombstone
    // before the record (the folder) goes; a failed write keeps the record
    if let Some(record) = &record {
        crate::record::retire(&host.home, record).map_err(after_stop)?;
    }
    let left = crate::mount_intents::end(open.as_ref().map_err(String::as_str), &bound);
    // the table is whole after a holder's panic (layerfs-project#177)
    crate::registry::SANDBOXES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&directory);
    std::fs::remove_dir_all(&directory).map_err(|error| after_stop(failed(error)))?;
    layerfs_home::home::generation::moved_above(&directory);
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Closed")),
        ("sandbox", SdkValue::text(id)),
        (
            "unknown",
            SdkValue::List(bound.into_iter().map(|b| SdkValue::Text(b.path)).collect()),
        ),
        ("left", SdkValue::List(left)),
    ]))
}

/// The Workspace id and Branch name a Workspace record keeps, when both
/// are readable.
fn recorded_bound(record: &crate::record::WorkspaceRecord) -> Option<([u8; 32], Branch)> {
    let id = layerfs_bridge::sdk_values::unhex(record.id()?)?;
    let branch = Branch::Named(record.branch()?.to_owned());
    Some((id.try_into().ok()?, branch))
}
