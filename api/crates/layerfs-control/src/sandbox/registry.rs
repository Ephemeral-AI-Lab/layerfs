//! What this process owns: its running Inits and its live Sandboxes. A
//! project itself is only its home entry, so a project removed there can be
//! initialized again without a restart.
//!
//! A Sandbox is one container and its daemon (or, with the local runtime,
//! one daemon process: `runtime`), known by its name. Its runtime runs
//! commands directly; its control connections (`pool`) each carry one
//! exchange at a time, and concurrent calls take separate connections.
use crate::common::outcome::refused;
use crate::MountedWorkspace;
use layerfs_bridge::control::{SdkRefusal, SdkValue, WorkspaceToken};
use layerfs_bridge::provision::StoreManifest;
use layerfs_sandbox::CommandIdentity;
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

/// One project, read from or saved to its home entry.
pub(crate) struct Project {
    pub(crate) store: layerfs_persistence::SealedStore,
    pub(crate) manifest: StoreManifest,
    /// The installed manifest once Import installed the Store in the volume.
    pub(crate) installed: Option<StoreManifest>,
}

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
    pub(crate) runtime: Option<Box<layerfs_sandbox::Sandbox>>,
    pub(crate) identity: CommandIdentity,
    pub(crate) controls: crate::daemon::pool::Controls,
    pub(crate) mounts: Mutex<BTreeMap<String, Mount>>,
}

/// This process's live Sandboxes, by their folder.
pub(crate) static SANDBOXES: LazyLock<Mutex<BTreeMap<std::path::PathBuf, Arc<SandboxState>>>> =
    LazyLock::new(Default::default);

/// A lock, refused when a holder panicked.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, SdkRefusal> {
    mutex
        .lock()
        .map_err(|_| refused("Failed", "binding state poisoned"))
}

/// A required text argument.
pub(crate) fn text<'a>(args: &'a SdkValue, name: &str) -> Result<&'a str, SdkRefusal> {
    args.get_text(name)
        .ok_or_else(|| refused("InvalidInput", format!("missing text argument `{name}`")))
}

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

/// The live Sandbox recorded at folder `directory`, when this process owns
/// it: a read model asks it for its measured capacity, never opening or
/// probing the Sandbox (#551 B8-157).
pub(crate) fn live_at(directory: &std::path::Path) -> Option<Arc<SandboxState>> {
    lock(&SANDBOXES).ok().and_then(|sandboxes| sandboxes.get(directory).cloned())
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
            crate::common::outcome::failed(failure)
        });
    }
    Ok(state)
}

impl SandboxState {
    /// Its runtime; a forwarded Sandbox's is its machine's, which this
    /// process does not run (its commands go through the worker).
    pub(crate) fn runtime(&self) -> Result<&layerfs_sandbox::Sandbox, SdkRefusal> {
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

/// 32 fresh random bytes from the transport's key generator.
pub(crate) fn random() -> Result<[u8; 32], SdkRefusal> {
    layerfs_bridge::duplex::generate_keypair()
        .map(|pair| pair.public)
        .map_err(crate::common::outcome::failed)
}
