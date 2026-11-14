//! The forced close of a `native` Sandbox, which bypasses Store and daemon
//! discovery and live unmount admission: only its runtime stops; the
//! machine, its Stores and saved history stay. A forced `machine.stop` on
//! `native` stops each Sandbox's runtime the same way ([`native_host`],
//! `Recorded::stop_now`).
use crate::host::{Deploy, Host, Message};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused, Outcome};
use std::path::Path;

/// The host operation's view of `native` Sandbox `sandbox` from its own
/// records alone: no Store, project or daemon is resolved.
pub fn native_host(home: &Path, sandbox: &str) -> Result<Host, SdkRefusal> {
    Ok(Host {
        home: home.to_path_buf(),
        store: String::new(),
        project: String::new(),
        sandbox: Some(sandbox.to_owned()),
        store_record: None,
        store_path: None,
        deploy: Some(Deploy {
            local_dir: Some(paths::run_root(home).display().to_string()),
            container_prefix: Some(format!("lfs-{}-", &home::opened(home)?.id[..8])),
            ..Deploy::default()
        }),
        postgresql: None,
        message: Message::Absent,
        started: None,
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

/// Forced `sandbox.close` of the Sandbox `host` names, on this process's
/// kernel: no daemon control call, Store open, active-Workspace refusal or
/// retry. The backend's Stop checks the runtime identity and ends its
/// mounts and processes; its Delete independently checks the physical
/// detach before the runtime goes. Answers each recorded or `held`
/// Workspace as `unknown`: its uncommitted changes end with the Sandbox.
pub fn close(host: &Host, held: &[String]) -> Outcome {
    let id = host.sandbox()?.to_owned();
    let directory = crate::files::sandbox_dir(host)?;
    let _guard = layerfs_store::project::discovery::guard(&directory)?;
    let value = crate::files::started(&directory)?
        .ok_or_else(|| refused("NoSuchSandbox", format!("no Sandbox {id} is recorded")))?;
    let mut unknown = crate::files::workspaces(&directory)?
        .into_iter()
        .map(|(path, record)| {
            record
                .path()
                .map(str::to_owned)
                .unwrap_or_else(|| path.display().to_string())
        })
        .collect::<Vec<_>>();
    for path in held {
        if !unknown.contains(path) {
            unknown.push(path.to_owned());
        }
    }
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
        .map_err(|error| after_stop(failed(error)))?;
    crate::registry::SANDBOXES
        .lock()
        .map_err(|_| after_stop(refused("Failed", "the Sandbox table's lock was poisoned")))?
        .remove(&directory);
    std::fs::remove_dir_all(&directory).map_err(|error| after_stop(failed(error)))?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Closed")),
        ("sandbox", SdkValue::text(id)),
        (
            "unknown",
            SdkValue::List(unknown.into_iter().map(SdkValue::Text).collect()),
        ),
    ]))
}
