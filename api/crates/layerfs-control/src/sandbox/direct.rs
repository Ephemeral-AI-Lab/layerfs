//! Workspace calls straight to a Sandbox's daemon (#535 G4, §7.2). A
//! `sandbox.create` that ran in a machine's worker answers the daemon's
//! endpoint in the machine and its control keys (`control`). Where the
//! machine's adapter carries that port to this host (`layerfs_machine::
//! forward`: an `ssh -L` on `ssh` and `lima`), and on `docker`, whose
//! Sandboxes publish their daemon's port on the host's loopback, the caller
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
use super::record::SandboxRecord;
use crate::common::context::Context;
use crate::common::home::{self, lease, paths};
use crate::common::outcome::{refused, Outcome};
use crate::machine::worker::Unreached;
use crate::sandbox::registry::{self, SandboxState, SANDBOXES};
use crate::sandbox::{dispatch, files};
use crate::store::call::store_call;
use crate::store::resolve::{self, Target};
use crate::workspace::{serve, unmount_guard};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::{BranchId, CommitId};
use std::net::SocketAddr;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The field of a Sandbox record's `daemon` that names its forward.
const FORWARD: &str = "forward";

/// The host address a Sandbox record's `daemon` reaches its daemon at, when
/// its port is forwarded to this host.
pub(crate) fn forward_of(daemon: &SdkValue) -> Option<SocketAddr> {
    daemon.get(FORWARD)?.get_text("address")?.parse().ok()
}

/// Whether the home reaches Sandbox `record`'s daemon directly.
pub(crate) fn forwarded(record: &SandboxRecord) -> bool {
    record
        .to_record()
        .get("daemon")
        .and_then(forward_of)
        .is_some()
}

/// Refuses Sandbox `id` of `home` (its record's `daemon`) while its
/// machine is stopped (it records no worker: nothing starts it, #535 D11),
/// and when the process that forwards its daemon's port has ended, matched
/// by id, start time and boot (never by id alone). A port nothing carries
/// (docker's) has no process to check.
pub(crate) fn check(home: &Path, id: &str, daemon: &SdkValue) -> Result<()> {
    let Some(forward) = daemon.get(FORWARD) else {
        return Ok(());
    };
    let machine = forward.get_text("machine").unwrap_or("");
    if !paths::worker_record(home, machine).exists() {
        return Err(refused(
            "Unavailable",
            format!(
                "machine {machine}, which runs Sandbox {id}, is not running; start it with                  `layerfs machine start {machine}`"
            ),
        ));
    }
    if forward.get_int("pid").is_none() || lease::recorded_alive(forward) {
        return Ok(());
    }
    Err(refused(
        "Unavailable",
        format!(
            "the forward to Sandbox {id}'s daemon (pid {}) has ended: machine {machine} \
             stopped or the forward was ended. Start the machine if it is stopped \
             (`layerfs machine start {machine}`), then `layerfs sandbox close {id}`",
            forward.get_int("pid").unwrap_or(0)
        ),
    ))
}

/// [`check`] of live Sandbox `state`, from its record.
pub(crate) fn live(state: &SandboxState) -> Result<()> {
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
pub(crate) fn unanswered(id: &str, daemon: &SdkValue, refusal: SdkRefusal) -> SdkRefusal {
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
pub(crate) fn with_keys(root: &Path, name: &str, out: SdkValue) -> Outcome {
    let keys = crate::common::secrets::daemon_keys(&paths::sandbox_dir(root, name))?
        .ok_or_else(|| refused("Failed", format!("Sandbox {name} has no daemon keys")))?;
    Ok(home::with_field(out, "control", keys))
}

/// What a create through a worker found about reaching its daemon from
/// here: the keys and the `daemon` field to record, or why it is reached
/// through the worker.
pub(crate) struct Opened {
    keys: Option<SdkValue>,
    daemon: Option<SdkValue>,
    /// A forward that failed: the Sandbox is reached through its worker.
    pub(crate) failed: Option<String>,
}

/// Opens the direct route to the daemon of Sandbox `record`, which its
/// machine's worker started and answered (`started`, with `control`):
/// a forward of its port where its machine's adapter makes one, docker's
/// published port as it is, else nothing (the worker route). Run before the
/// home and Sandbox locks are taken; [`save`] records it under them.
pub(crate) fn open(home: &Path, record: &SandboxRecord, started: &SdkValue) -> Opened {
    let mut opened = Opened {
        keys: None,
        daemon: None,
        failed: None,
    };
    let Some(keys) = started.get("control") else {
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
    let mut fields = vec![
        ("address", SdkValue::text(forward.0.to_string())),
        ("machine", SdkValue::text(record.machine())),
    ];
    if let Some(pid) = forward.1 {
        fields.push(("pid", SdkValue::Int(i64::from(pid))));
        // the forward's identity, so a reused id is never taken for it
        if let Ok(process) = layerfs_sandbox::local::LocalProcess::observe(pid) {
            fields.push(("pid_start", SdkValue::text(process.start.to_string())));
            if let Some(boot) = process.boot {
                fields.push(("pid_boot", SdkValue::text(format!("{boot:032x}"))));
            }
        }
    }
    let forward = SdkValue::Map(
        (fields.into_iter())
            .map(|(n, v)| (n.to_owned(), v))
            .collect(),
    );
    let copied = |name: &str| started.get(name).cloned().unwrap_or_default();
    opened.daemon = Some(SdkValue::map([
        (FORWARD, forward),
        ("name", copied("name")),
        ("runtime", copied("runtime")),
        ("platform", copied("platform")),
        ("image", copied("image")),
    ]));
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
pub(crate) fn save(home: &Path, record: SandboxRecord, opened: &Opened) -> Result<SandboxRecord> {
    let (Some(keys), Some(daemon)) = (&opened.keys, &opened.daemon) else {
        return Ok(record);
    };
    if let Err(refusal) = crate::common::secrets::save_daemon_keys(&record.dir(home), keys) {
        end(home, record.machine(), daemon);
        return Err(refusal);
    }
    Ok(record.with("daemon", daemon.clone()))
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
    use layerfs_machine::{Code, Platform};
    let record = crate::machine::record::MachineRecord::of(home, machine)?;
    // a docker Sandbox publishes its daemon's port on the host's loopback,
    // which this host reaches as it is (macOS through Docker Desktop)
    if record.platform()? == Platform::Docker {
        return Ok(Some((endpoint, None)));
    }
    let spec = crate::machine::machine_spec(home, &home::open(home)?.id, &record)?;
    match layerfs_machine::forward(&spec, endpoint.port()) {
        Ok(forward) => Ok(Some((forward.address, forward.pid))),
        Err(error) if error.code == Code::Unsupported => Ok(None),
        Err(error) => Err(crate::machine::machine_refusal(error)),
    }
}

/// Without the machine adapters (a guest worker's build) nothing is
/// forwarded.
#[cfg(not(feature = "machine"))]
fn carried(_: &Path, _: &str, _: SocketAddr) -> Result<Option<(SocketAddr, Option<u32>)>> {
    Ok(None)
}

/// Ends the forward a record's `daemon` names, if it still is that
/// process (`layerfs_machine::forward_close` checks its command line too).
fn end(home: &Path, machine: &str, daemon: &SdkValue) {
    let Some(forward) = daemon.get(FORWARD) else {
        return;
    };
    let Some(pid) = forward.get_int("pid").and_then(|p| u32::try_from(p).ok()) else {
        return;
    };
    if !lease::recorded_alive(forward) {
        return;
    }
    #[cfg(feature = "machine")]
    {
        let Some(address) = forward_of(daemon) else {
            return;
        };
        let spec = crate::machine::record::MachineRecord::of(home, machine)
            .and_then(|record| crate::machine::machine_spec(home, &home::open(home)?.id, &record));
        if let Ok(spec) = spec {
            let forward = layerfs_machine::Forward {
                address,
                pid: Some(pid),
            };
            layerfs_machine::forward_close(&spec, &forward);
        }
    }
    #[cfg(not(feature = "machine"))]
    let _ = (home, machine, pid);
}

/// After Sandbox `record` closed (or was found gone): this process forgets
/// it and its forward is ended. Its folder is the caller's to remove.
pub(crate) fn release(home: &Path, record: &SandboxRecord) {
    let dir = record.dir(home);
    if let Ok(mut live) = registry::lock(&SANDBOXES) {
        live.remove(&dir);
    }
    if let Some(daemon) = record.to_record().get("daemon") {
        end(home, record.machine(), daemon);
    }
}

/// The host operation arguments of a direct call on Sandbox `record` of
/// `target`, plus `more`: the home is its serving side.
fn args(
    home: &Path,
    record: &SandboxRecord,
    target: &Target,
    more: Vec<(&str, SdkValue)>,
) -> SdkValue {
    let mut fields = vec![
        ("home", SdkValue::text(home.display().to_string())),
        ("sandbox", SdkValue::text(record.name())),
        ("store", SdkValue::text(&target.store)),
        ("project", SdkValue::text(&target.project)),
    ];
    fields.extend(more);
    SdkValue::Map(
        (fields.into_iter())
            .map(|(n, v)| (n.to_owned(), v))
            .collect(),
    )
}

/// Runs Workspace operation `op` (`workspace.mount`, `workspace.commit`,
/// `workspace.unmount`, `backend.daemon.workspace.status`,
/// `sandbox.list_workspaces`) on forwarded Sandbox `record` in this
/// process, under the Store's lease.
pub(crate) fn call(
    context: &mut Context<'_>,
    record: &SandboxRecord,
    op: &str,
    more: Vec<(&str, SdkValue)>,
) -> Outcome {
    let home = context.home.clone();
    let target = resolve::target(&home, record.project().unwrap_or(""))?;
    let _lease = lease::hold(&home, &target.store)?;
    let args = args(&home, record, &target, more);
    match op {
        "workspace.mount" => serve::mount_with(&args, &mut |branch| {
            Ok(branch_of(context, &target, branch)?.0)
        }),
        "workspace.commit" => serve::commit(&serve::by_name(args)?),
        "workspace.unmount" => serve::unmount(&serve::by_name(args)?),
        "backend.daemon.workspace.status" => {
            serve::status_with(&serve::by_name(args)?, &mut |id, name| {
                let (found, head) = branch_of(context, &target, name?).ok()?;
                (found == id).then_some(head)
            })
        }
        "sandbox.list_workspaces" | layerfs_bridge::control::CAPACITY_OPERATION => {
            dispatch::call(op, args)
        }
        _ => Err(refused(
            "InvalidInput",
            format!("{op} is not sent to a Sandbox's daemon directly"),
        )),
    }
}

/// `sandbox.get`'s Workspace list of forwarded Sandbox `record`: a daemon
/// that cannot be reached is [`Unreached`], which the get reports as down.
pub(crate) fn list(
    context: &mut Context<'_>,
    record: &SandboxRecord,
) -> std::result::Result<Outcome, Unreached> {
    match call(context, record, "sandbox.list_workspaces", vec![]) {
        Err(refusal) if refusal.code == "Unavailable" => Err(Unreached(refusal)),
        other => Ok(other),
    }
}

/// The id and head of Branch `name` of `target`'s project, read through
/// the Store's own route (in this process, or its machine's worker).
fn branch_of(
    context: &mut Context<'_>,
    target: &Target,
    name: &str,
) -> Result<(BranchId, Option<CommitId>)> {
    let listed = store_call(
        context,
        target,
        "project.list_branches",
        vec![("name".to_owned(), SdkValue::text(name))],
    )?;
    let found = match listed.get("items") {
        Some(SdkValue::List(items)) => items.first().cloned(),
        _ => None,
    };
    let id = found
        .as_ref()
        .and_then(|item| item.get_text("id"))
        .and_then(layerfs_bridge::sdk_values::unhex)
        .and_then(|bytes| BranchId::from_slice(&bytes).ok())
        .ok_or_else(|| refused("NoSuchBranch", format!("Branch {name} does not exist")))?;
    let head = found
        .as_ref()
        .and_then(|item| item.get_text("head"))
        .and_then(layerfs_bridge::sdk_values::unhex)
        .and_then(|bytes| CommitId::from_slice(&bytes).ok());
    Ok((id, head))
}

/// The directory of Workspace `at` (its path, or its name) of forwarded
/// Sandbox `record`, as its record in the home folder names it, for an
/// exec its worker runs: refused `NotMounted` when no record names it,
/// and while an unmount holds it.
pub(crate) fn exec_directory(home: &Path, record: &SandboxRecord, at: &str) -> Result<String> {
    let dir = record.dir(home);
    let field = if at.starts_with('/') { "path" } else { "name" };
    let found = (files::workspaces(&dir)?.into_iter())
        .find(|(_, workspace)| workspace.get_text(field) == Some(at));
    let Some((_, workspace)) = found else {
        return Err(refused(
            "NotMounted",
            format!("{}:{at} is not mounted", record.name()),
        ));
    };
    let path = registry::text(&workspace, "path")?.to_owned();
    unmount_guard::check_in(&dir, &path)?;
    Ok(path)
}

/// The Workspaces forwarded Sandbox `record` holds, which its close sends
/// its worker (`held`): those its daemon still has, checked under the
/// folder's turn with no unmount in progress; when the daemon cannot be
/// reached, every Workspace recorded in the home folder.
pub(crate) fn held(home: &Path, record: &SandboxRecord) -> Result<Vec<String>> {
    let target = resolve::target(home, record.project().unwrap_or(""))?;
    let args = args(home, record, &target, vec![]);
    let Ok(state) = registry::sandbox_live(&args) else {
        return (files::workspaces(&record.dir(home))?.iter())
            .map(|(_, workspace)| Ok(registry::text(workspace, "path")?.to_owned()))
            .collect();
    };
    let _guard = files::guard(&state.directory)?;
    let recoverable = unmount_guard::check_all(&state)?;
    files::refresh_except(&state, &recoverable)?;
    let mounted = registry::lock(&state.mounts)?
        .keys()
        .filter(|path| !recoverable.contains(path))
        .cloned()
        .collect();
    Ok(mounted)
}
