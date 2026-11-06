//! What this process owns: its running Inits and its live Sandboxes. A
//! project itself is only its home entry, so a project removed there can be
//! initialized again without a restart.
//!
//! A Sandbox is one container and its daemon (or, with the local runtime,
//! one daemon process: `runtime`), known by its name. Its runtime runs
//! commands directly; its control connections (`pool`) each carry one
//! exchange at a time, and concurrent calls take separate connections.
use crate::MountedWorkspace;
use layerfs_bridge::control::{SdkRefusal, SdkValue, WorkspaceToken};
use layerfs_home::helpers::lock;
use layerfs_home::outcome::refused;
use layerfs_isolation::CommandIdentity;
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};

/// One mounted Workspace.
pub(crate) struct Mount {
    pub(crate) workspace: MountedWorkspace,
    pub(crate) branch: String,
}

/// One live Sandbox: Sandbox `id` (its name) of `project`.
pub(crate) struct SandboxState {
    pub(crate) id: String,
    pub(crate) project: String,
    /// Its container's name, `<prefix><id>` (a local Sandbox's directory
    /// name).
    pub(crate) name: String,
    /// Its folder, `<root>/sandbox/<id>/`.
    pub(crate) directory: std::path::PathBuf,
    /// Its runtime, observed where it runs; `None` for a Sandbox this
    /// process reaches only through its daemon's forwarded port (#535 G4),
    /// whose runtime is in its machine.
    pub(crate) runtime: Option<Box<layerfs_isolation::Sandbox>>,
    pub(crate) identity: CommandIdentity,
    pub(crate) controls: crate::daemon::pool::Controls,
    pub(crate) mounts: Mutex<BTreeMap<String, Mount>>,
}

/// This process's live Sandboxes, by their folder.
pub(crate) static SANDBOXES: LazyLock<Mutex<BTreeMap<std::path::PathBuf, Arc<SandboxState>>>> =
    LazyLock::new(Default::default);

/// The Sandbox an operation names, with this process's mount and process
/// tables as they stand: no daemon call (#352 F-029). A call that names a
/// mount checks just that mount (`SandboxState::token`, `directory`); one
/// that needs every mount or process another process may have changed takes
/// `discovery::refresh`.
pub(crate) fn sandbox(args: &SdkValue) -> Result<Arc<SandboxState>, SdkRefusal> {
    let directory = crate::sandbox::files::sandbox_dir(args)?;
    let existing = lock(&SANDBOXES)?.get(&directory).cloned();
    match existing {
        Some(state) => Ok(state),
        None => {
            let state = crate::sandbox::files::reopen(args)?;
            lock(&SANDBOXES)?.insert(directory, state.clone());
            Ok(state)
        }
    }
}

/// The Sandbox `args` name when its container still runs it: this
/// process's state, checked with one Engine inspection, or reopened from its
/// records (which inspects too). A container found gone or stopped (an idle
/// Sandbox's daemon ends itself, #373) is the refusal of
/// `discovery::gone`, and this process forgets the state.
pub(crate) fn sandbox_live(args: &SdkValue) -> Result<Arc<SandboxState>, SdkRefusal> {
    let directory = crate::sandbox::files::sandbox_dir(args)?;
    let Some(state) = lock(&SANDBOXES)?.get(&directory).cloned() else {
        return sandbox(args);
    };
    let Some(runtime) = &state.runtime else {
        // a forwarded Sandbox: its forward must still run (#535 G4)
        if let Err(refusal) = crate::sandbox::direct::live(&state) {
            lock(&SANDBOXES)?.remove(&directory);
            return Err(refusal);
        }
        return Ok(state);
    };
    if let Err(failure) = runtime.endpoint() {
        lock(&SANDBOXES)?.remove(&directory);
        return Err(if failure.container_gone() || failure.container_stopped() {
            crate::sandbox::recorded::gone(
                &state.id,
                &runtime.identity().to_string(),
                failure.container_stopped(),
            )
        } else {
            layerfs_home::outcome::failed(failure)
        });
    }
    Ok(state)
}

impl SandboxState {
    /// Its runtime; a forwarded Sandbox's is its machine's, which this
    /// process does not run (its commands go through the worker).
    pub(crate) fn runtime(&self) -> Result<&layerfs_isolation::Sandbox, SdkRefusal> {
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
    pub(crate) fn token(&self, path: &str) -> Result<WorkspaceToken, SdkRefusal> {
        self.mounted(path, |mount| mount.workspace.bound.token)
    }

    /// The kernel directory of the Workspace mounted at `path`.
    pub(crate) fn directory(&self, path: &str) -> Result<String, SdkRefusal> {
        self.mounted(path, |mount| mount.workspace.ready.directory.clone())
    }

    /// `pick` from the mount at `path`. A mount this process knows is used
    /// while its shared record exists. Otherwise (an unknown path, or a
    /// known one another process unmounted) the shared records are read
    /// once, for whatever is mounted there now; the full refresh replaces
    /// the table, so a stale entry goes with it.
    fn mounted<T>(&self, path: &str, pick: impl Fn(&Mount) -> T) -> Result<T, SdkRefusal> {
        crate::workspace::unmount_guard::check(self, path)?;
        let known = lock(&self.mounts)?
            .get(path)
            .map(|mount| (mount.workspace.bound.token, pick(mount)));
        match known {
            Some((token, value)) if crate::sandbox::files::workspace_file(self, token).exists() => {
                return Ok(value)
            }
            _ => crate::sandbox::files::refresh(self)?,
        }
        lock(&self.mounts)?
            .get(path)
            .map(pick)
            .ok_or_else(|| refused("NotMounted", format!("{}:{path} is not mounted", self.id)))
    }
}
