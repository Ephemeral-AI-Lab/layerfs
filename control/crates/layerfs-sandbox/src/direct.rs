//! Workspace calls straight to a Sandbox's daemon (#535 G4, §7.2). A
//! `sandbox.create` that ran in a machine's worker answers the daemon's
//! endpoint in the machine and its control keys (`control`). Where the
//! machine's adapter carries that port to this host (`layerfs_machine::platform::
//! forward`: an `ssh -L` on `lima`), the caller
//! keeps the keys in the home's `sandbox/<name>/control.json` (0600) and the
//! forward in its record's `daemon.forward`. The home's folder then serves
//! the Sandbox like `native`'s: `workspace.mount`, `commit`, `unmount`,
//! `status` and `sandbox.get`'s Workspace list dial the daemon from this
//! process, with the Workspace records, unmount guards and turns in that
//! folder. A Branch lookup or head read goes the Store's own route.
//! `workspace.exec` runs `bash -c` through the Sandbox's runtime, which is
//! in the machine: it still goes through the worker, with the directory
//! checked here. Create, close and a machine with no forward use the
//! worker as before.
use super::record::{DaemonRecord, Forward, SandboxRecord};
use crate::host::{Call, Host, Message, Started};
use crate::registry::{self, SandboxState, SANDBOXES};
use crate::unmount_guard;
use crate::{dispatch, files};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::helpers;
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_machine::worker::Unreached;
use layerfs_store::lease;
use layerfs_store::resolve::{self, Target};
use std::net::SocketAddr;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The host address a Sandbox record's `daemon` reaches its daemon at, when
/// its port is forwarded to this host.
pub fn forward_of(daemon: &DaemonRecord) -> Option<SocketAddr> {
    daemon.forward()?.address()
}

/// Whether the home reaches Sandbox `record`'s daemon directly.
pub fn forwarded(record: &SandboxRecord) -> bool {
    record.daemon().and_then(forward_of).is_some()
}

/// Refuses Sandbox `id` of `home` (its record's `daemon`) while its
/// machine is stopped (it records no worker: nothing starts it, #535 D11),
/// and when the process that forwards its daemon's port has ended, matched
/// by id, start time and boot (never by id alone). A port nothing carries
/// has no process to check.
pub fn check(home: &Path, id: &str, daemon: &DaemonRecord) -> Result<()> {
    let Some(forward) = daemon.forward() else {
        return Ok(());
    };
    let machine = forward.machine().unwrap_or("");
    if !paths::worker_record(home, machine).exists() {
        return Err(refused(
            "Unavailable",
            format!(
                "machine {machine}, which runs Sandbox {id}, is not running; start it with                  `layerfs machine start {machine}`"
            ),
        ));
    }
    if forward.pid().is_none() || forward.alive() {
        return Ok(());
    }
    Err(refused(
        "Unavailable",
        format!(
            "the forward to Sandbox {id}'s daemon (pid {}) has ended: machine {machine} \
             stopped or the forward was ended. Start the machine if it is stopped \
             (`layerfs machine start {machine}`), then `layerfs sandbox close {id}`",
            forward.pid().unwrap_or(0)
        ),
    ))
}

/// [`check`] of live Sandbox `state`, from its record.
pub fn live(state: &SandboxState) -> Result<()> {
    let home = (state.directory.parent())
        .and_then(Path::parent)
        .ok_or_else(|| refused("Failed", "a Sandbox folder is in a home"))?;
    match files::started(&state.directory)? {
        Some(daemon) => check(home, &state.id, &daemon),
        None => Err(refused(
            "NoSuchSandbox",
            format!("Sandbox {} was closed", state.id),
        )),
    }
}

/// The refusal of a forwarded Sandbox `id` whose daemon did not answer a
/// new connection: `Unavailable`, naming the calls that say why. A busy
/// daemon's `Busy` and a restarted one's `StaleSandbox` stay as they are.
pub fn unanswered(id: &str, daemon: &DaemonRecord, refusal: SdkRefusal) -> SdkRefusal {
    if matches!(refusal.code.as_str(), "Busy" | "StaleSandbox") {
        return refusal;
    }
    let at = forward_of(daemon)
        .map(|a| a.to_string())
        .unwrap_or_default();
    refused(
        "Unavailable",
        format!(
            "Sandbox {id}'s daemon does not answer at {at}: {}; `layerfs sandbox get {id}` \
             shows whether it runs, `layerfs sandbox close {id}` closes it",
            refusal.message
        ),
    )
}

/// A worker's `sandbox.create` answer `out` for Sandbox `name` under its
/// `root`, with the daemon's endpoint in the machine and its control keys
/// (`control`), which only the caller reads.
pub fn with_keys(root: &Path, name: &str, out: SdkValue) -> Outcome {
    let keys = layerfs_home::secrets::daemon_keys(&paths::sandbox_dir(root, name))?
        .ok_or_else(|| refused("Failed", format!("Sandbox {name} has no daemon keys")))?;
    Ok(home::with_field(out, "control", keys))
}

/// What a create through a worker found about reaching its daemon from
/// here: the keys and the `daemon` field to record, or why it is reached
/// through the worker.
pub struct Opened {
    keys: Option<SdkValue>,
    daemon: Option<DaemonRecord>,
    /// A forward that failed: the Sandbox is reached through its worker.
    pub failed: Option<String>,
}

/// Opens the direct route to the daemon of Sandbox `record`, which its
/// machine's worker started and answered (`started`, with `control`):
/// a forward of its port where its machine's adapter makes one, else
/// nothing (the worker route; `docker` among them, D-8). Run before the
/// home and Sandbox locks are taken; [`save`] records it under them.
pub fn open(home: &Path, record: &SandboxRecord, started: &Started) -> Opened {
    let mut opened = Opened {
        keys: None,
        daemon: None,
        failed: None,
    };
    let Some(keys) = &started.control else {
        return opened;
    };
    let Some(endpoint) = keys
        .get_text("endpoint")
        .and_then(|e| e.parse::<SocketAddr>().ok())
    else {
        return opened;
    };
    let forward = match carried(home, record.machine(), endpoint) {
        Ok(Some(forward)) => forward,
        Ok(None) => return opened,
        Err(refusal) => {
            opened.failed = Some(refusal.message);
            return opened;
        }
    };
    // the forward's identity, so a reused id is never taken for it
    let process = forward.1.map(|pid| {
        let observed = layerfs_bridge::process::LocalProcess::observe(pid).ok();
        (pid, observed.map(|process| (process.start, process.boot)))
    });
    let forward = Forward::new(forward.0, record.machine(), process);
    opened.daemon = Some(DaemonRecord::forwarded(forward, started));
    opened.keys = Some(SdkValue::map([
        ("endpoint", SdkValue::text(endpoint_of(&opened))),
        ("secret", keys.get("secret").cloned().unwrap_or_default()),
        ("peer", keys.get("peer").cloned().unwrap_or_default()),
        (
            "instance",
            keys.get("instance").cloned().unwrap_or_default(),
        ),
    ]));
    opened
}

fn endpoint_of(opened: &Opened) -> String {
    (opened.daemon.as_ref())
        .and_then(forward_of)
        .map(|a| a.to_string())
        .unwrap_or_default()
}

/// Records what [`open`] opened in Sandbox `record`'s home folder: the
/// keys in `control.json` first, then `record` with its `daemon`. A forward
/// whose keys could not be written is ended.
pub fn save(home: &Path, record: SandboxRecord, opened: &Opened) -> Result<SandboxRecord> {
    let (Some(keys), Some(daemon)) = (&opened.keys, &opened.daemon) else {
        return Ok(record);
    };
    if let Err(refusal) = layerfs_home::secrets::save_daemon_keys(&record.dir(home), keys) {
        end(home, record.machine(), daemon);
        return Err(refusal);
    }
    Ok(record.with_daemon(daemon.clone()))
}

/// The address machine `machine` carries its Sandbox daemon's `endpoint`
/// to on this host, and the forward's process: `None` where its adapter
/// forwards nothing (the worker route stays).
#[cfg(feature = "machine")]
fn carried(
    home: &Path,
    machine: &str,
    endpoint: SocketAddr,
) -> Result<Option<(SocketAddr, Option<u32>)>> {
    use layerfs_machine::platform::Code;
    let record = layerfs_machine::record::MachineRecord::of(home, machine)?;
    let spec = layerfs_machine::machine_spec(home, &layerfs_home::home::opened(home)?.id, &record)?;
    match layerfs_machine::platform::forward(&spec, endpoint.port()) {
        Ok(forward) => Ok(Some((forward.address, forward.pid))),
        Err(error) if error.code == Code::Unsupported => Ok(None),
        Err(error) => Err(layerfs_machine::machine_refusal(error)),
    }
}

/// Without the machine adapters (a guest worker's build) nothing is
/// forwarded.
#[cfg(not(feature = "machine"))]
fn carried(_: &Path, _: &str, _: SocketAddr) -> Result<Option<(SocketAddr, Option<u32>)>> {
    Ok(None)
}

/// Ends the forward a record's `daemon` names, if it still is that
/// process (`layerfs_machine::platform::forward_close` checks its command line too).
fn end(home: &Path, machine: &str, daemon: &DaemonRecord) {
    let Some(forward) = daemon.forward() else {
        return;
    };
    let Some(pid) = forward.pid().and_then(|p| u32::try_from(p).ok()) else {
        return;
    };
    if !forward.alive() {
        return;
    }
    #[cfg(feature = "machine")]
    {
        let Some(address) = forward_of(daemon) else {
            return;
        };
        let spec = layerfs_machine::record::MachineRecord::of(home, machine).and_then(|record| {
            layerfs_machine::machine_spec(home, &layerfs_home::home::opened(home)?.id, &record)
        });
        if let Ok(spec) = spec {
            let forward = layerfs_machine::platform::Forward {
                address,
                pid: Some(pid),
            };
            layerfs_machine::platform::forward_close(&spec, &forward);
        }
    }
    #[cfg(not(feature = "machine"))]
    let _ = (home, machine, pid);
}

/// After Sandbox `record` closed (or was found gone): this process forgets
/// it and its forward is ended. Its folder is the caller's to remove.
pub fn release(home: &Path, record: &SandboxRecord) {
    let dir = record.dir(home);
    if let Ok(mut live) = helpers::lock(&SANDBOXES) {
        live.remove(&dir);
    }
    if let Some(daemon) = record.daemon() {
        end(home, record.machine(), daemon);
    }
}

/// The host request of a direct call on Sandbox `record` of `target`, with
/// `message`: the home is its serving side, with no Store record, path or
/// deployment.
pub fn host(home: &Path, record: &SandboxRecord, target: &Target, message: Message) -> Host {
    Host {
        home: home.to_path_buf(),
        store: target.store.clone(),
        project: target.project.clone(),
        sandbox: Some(record.name().to_owned()),
        store_record: None,
        store_path: None,
        deploy: None,
        postgresql: None,
        message,
        started: None,
    }
}

/// `sandbox.get`'s Workspace list of forwarded Sandbox `record`: a daemon
/// that cannot be reached is [`Unreached`], which the get reports as down.
pub fn list(home: &Path, record: &SandboxRecord) -> std::result::Result<Outcome, Unreached> {
    match served(home, record, Call::ListWorkspaces { path: None }) {
        Err(refusal) if refusal.code == "Unavailable" => Err(Unreached(refusal)),
        other => Ok(other),
    }
}

/// The daemon's control-capacity read model of forwarded Sandbox `record`
/// (#540), on the connection the Workspace list lent from this process's
/// pool for it (#543). A refused or unanswered read is the caller's to omit.
pub fn capacity(home: &Path, record: &SandboxRecord) -> Outcome {
    served(home, record, Call::Capacity)
}

/// Host `call` on forwarded Sandbox `record` in this process, under the
/// Store's lease (the Sandbox half of `workspace::direct::call`).
fn served(home: &Path, record: &SandboxRecord, call: Call) -> Outcome {
    let home = home.to_path_buf();
    let target = resolve::target(&home, record.project().unwrap_or(""))?;
    let _lease = lease::hold(&home, &target.store)?;
    dispatch::call(host(&home, record, &target, Message::Absent), call)
}

/// The Workspaces forwarded Sandbox `record` holds, which its close sends
/// its worker (`held`): those its daemon still has, checked under the
/// folder's turn with no unmount in progress; when the daemon cannot be
/// reached, every Workspace recorded in the home folder.
pub fn held(home: &Path, record: &SandboxRecord) -> Result<Vec<String>> {
    let target = resolve::target(home, record.project().unwrap_or(""))?;
    let host = host(home, record, &target, Message::Absent);
    let Ok(state) = registry::sandbox_live(&host) else {
        return (files::workspaces(&record.dir(home))?.iter())
            .map(|(_, workspace)| Ok(helpers::required(workspace.path(), "path")?.to_owned()))
            .collect();
    };
    let _guard = layerfs_store::project::discovery::guard(&state.directory)?;
    let recoverable = unmount_guard::check_all(&state)?;
    files::refresh_except(&state, &recoverable)?;
    let mounted = helpers::lock(&state.mounts)?
        .keys()
        .filter(|path| !recoverable.contains(path))
        .cloned()
        .collect();
    Ok(mounted)
}
