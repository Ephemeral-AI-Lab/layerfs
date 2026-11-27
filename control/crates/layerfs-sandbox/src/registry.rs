//! What this process owns: its running Inits and its live Sandboxes. A
//! project itself is only its home entry, so a project deleted there can be
//! initialized again (under another name, until a compaction frees the name).
//!
//! A Sandbox is one daemon process (`runtime`: native, or namespaces with
//! its init stage), known by its name. Its runtime runs
//! commands directly; its control connections (`pool`) each carry one
//! exchange at a time, and concurrent calls take separate connections.
use crate::host::Host;
use crate::CommandIdentity;
use crate::MountedWorkspace;
use layerfs_home::helpers::lock;
use layerfs_home::outcome::refused;
use layerfs_protocol::daemon::WorkspaceToken;
use layerfs_protocol::sdk::SdkRefusal;
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};

/// One mounted Workspace.
pub struct Mount {
    pub workspace: MountedWorkspace,
    pub branch: String,
}

/// One live Sandbox: Sandbox `id` (its name) of `project`.
pub struct SandboxState {
    pub id: String,
    pub project: String,
    /// Its runtime name, `<prefix><id>` (its directory's name).
    pub name: String,
    /// Its folder, `<root>/sandbox/<id>/`.
    pub directory: std::path::PathBuf,
    /// Its runtime, observed where it runs; `None` for a Sandbox this
    /// process reaches only through its daemon's forwarded port (#535 G4),
    /// whose runtime is in its machine.
    pub runtime: Option<Box<crate::Sandbox>>,
    pub identity: CommandIdentity,
    pub controls: crate::daemon::pool::Controls,
    pub mounts: Mutex<BTreeMap<String, Mount>>,
}

/// This process's live Sandboxes, by their folder.
pub static SANDBOXES: LazyLock<Mutex<BTreeMap<std::path::PathBuf, Arc<SandboxState>>>> =
    LazyLock::new(Default::default);

/// The Sandbox an operation names, with this process's mount and process
/// tables as they stand: no daemon call (#352 F-029). A call that names a
/// mount checks just that mount (`SandboxState::token`, `directory`); one
/// that needs every mount or process another process may have changed takes
/// `discovery::refresh`.
pub fn sandbox(host: &Host) -> Result<Arc<SandboxState>, SdkRefusal> {
    let directory = crate::files::sandbox_dir(host)?;
    let existing = lock(&SANDBOXES)?.get(&directory).cloned();
    match existing {
        Some(state) => Ok(state),
        None => {
            let state = crate::files::reopen(host)?;
            lock(&SANDBOXES)?.insert(directory, state.clone());
            Ok(state)
        }
    }
}

/// The Sandbox `host` names when its daemon still runs: this process's
/// state, checked with one inspection, or reopened from its records (which
/// inspects too). A daemon found gone or stopped (an idle Sandbox's daemon
/// ends itself, #373) is the refusal of
/// `discovery::gone`, and this process forgets the state.
pub fn sandbox_live(host: &Host) -> Result<Arc<SandboxState>, SdkRefusal> {
    let directory = crate::files::sandbox_dir(host)?;
    let Some(state) = lock(&SANDBOXES)?.get(&directory).cloned() else {
        return sandbox(host);
    };
    let Some(runtime) = &state.runtime else {
        // a forwarded Sandbox: its forward must still run (#535 G4)
        if let Err(refusal) = crate::direct::live(&state) {
            lock(&SANDBOXES)?.remove(&directory);
            return Err(refusal);
        }
        return Ok(state);
    };
    if let Err(failure) = runtime.endpoint() {
        lock(&SANDBOXES)?.remove(&directory);
        return Err(if failure.container_stopped() {
            crate::reopen::gone(&state.id, &runtime.identity().to_string(), true)
        } else {
            crate::daemon::values::wire(failure)
        });
    }
    Ok(state)
}

/// How long ending a process's idle daemon sessions may take in all when it
/// is about to exit: a daemon that does not answer in time is left a dropped
/// connection, as before.
pub const SESSION_END: std::time::Duration = std::time::Duration::from_millis(250);

/// Ends the idle daemon sessions of every Sandbox this process reached
/// (`Controls::end_idle`), all inside `within`: what a one-shot process does
/// when its call is over, so its exit is not a dropped connection in each
/// daemon's log. It takes no record lock, never fails and sends nothing
/// twice. Answers how many sessions ended.
pub fn end_idle_sessions(within: std::time::Duration) -> usize {
    let deadline = std::time::Instant::now() + within;
    // the table is held only for single steps, and is whole after a
    // holder's panic (layerfs-project#177)
    let live: Vec<Arc<SandboxState>> = SANDBOXES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .values()
        .cloned()
        .collect();
    live.iter()
        .map(|state| state.controls.end_idle(deadline))
        .sum()
}

impl SandboxState {
    /// Its runtime; a forwarded Sandbox's is its machine's, which this
    /// process does not run (its commands go through the worker).
    pub fn runtime(&self) -> Result<&crate::Sandbox, SdkRefusal> {
        self.runtime.as_deref().ok_or_else(|| {
            refused(
                "Failed",
                format!(
                    "Sandbox {}'s runtime is in its machine; this process reaches only its daemon",
                    self.id
                ),
            )
        })
    }

    /// The token of the Workspace mounted at `path`.
    pub fn token(&self, path: &str) -> Result<WorkspaceToken, SdkRefusal> {
        crate::unmount_guard::check(self, path)?;
        self.mounted(path, |mount| mount.workspace.bound.token)
    }

    /// The token of the Workspace at `path` after unmount admission checked it.
    pub fn unmount_token(&self, path: &str) -> Result<WorkspaceToken, SdkRefusal> {
        self.mounted(path, |mount| mount.workspace.bound.token)
    }

    /// The kernel directory of the Workspace mounted at `path`.
    pub fn directory(&self, path: &str) -> Result<String, SdkRefusal> {
        crate::unmount_guard::check(self, path)?;
        self.mounted(path, |mount| mount.workspace.ready.directory.clone())
    }

    /// `pick` from the mount at `path`. A mount this process knows is used
    /// while its shared record exists. Otherwise (an unknown path, or a
    /// known one another process unmounted) the shared records are read
    /// once, for whatever is mounted there now; the full refresh replaces
    /// the table, so a stale entry goes with it.
    fn mounted<T>(&self, path: &str, pick: impl Fn(&Mount) -> T) -> Result<T, SdkRefusal> {
        let known = lock(&self.mounts)?
            .get(path)
            .map(|mount| (mount.workspace.bound.token, pick(mount)));
        match known {
            Some((token, value)) if crate::files::workspace_file(self, token).exists() => {
                return Ok(value)
            }
            _ => crate::files::refresh(self)?,
        }
        lock(&self.mounts)?
            .get(path)
            .map(pick)
            .ok_or_else(|| refused("NotMounted", format!("{}:{path} is not mounted", self.id)))
    }
}
