//! What this process owns: its running Inits and its live Sandboxes. A
//! project itself is only its home entry (`<home>/<name>/`), so a project
//! removed there can be initialized again without a restart.
//!
//! A Sandbox is one container and its daemon. Its runtime runs commands
//! directly; its control connections (`pool`) each carry one exchange at a
//! time, and concurrent calls take separate connections. The Sandbox runtime owner (this
//! binding) also keeps the background processes it started; the daemon holds
//! no process record.
use crate::job::refused;
#[cfg(unix)]
use crate::pool::Controls;
#[cfg(unix)]
use layerfs_api::MountedWorkspace;
#[cfg(unix)]
use layerfs_bridge::control::WorkspaceToken;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::provision::StoreManifest;
#[cfg(unix)]
use layerfs_sandbox::{backend::docker::Sandbox, CommandIdentity};
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
#[cfg(unix)]
pub(crate) struct Mount {
    pub(crate) workspace: MountedWorkspace,
    pub(crate) branch: String,
}

/// One background process started in a Workspace.
#[cfg(unix)]
pub(crate) struct Process {
    pub(crate) path: String,
    pub(crate) command: String,
    pub(crate) pid: u32,
    pub(crate) stopped: bool,
    pub(crate) start: String,
}

/// One live Sandbox: Sandbox `id` (its number) of `project`.
#[cfg(unix)]
pub(crate) struct SandboxState {
    pub(crate) id: String,
    pub(crate) project: String,
    /// Its container's name, `<prefix><project>-<id>`.
    pub(crate) name: String,
    pub(crate) directory: std::path::PathBuf,
    pub(crate) runtime: Sandbox,
    pub(crate) identity: CommandIdentity,
    pub(crate) controls: Controls,
    pub(crate) mounts: Mutex<BTreeMap<String, Mount>>,
    pub(crate) processes: Mutex<BTreeMap<String, Process>>,
}

/// The live counts of each Init still running, by project name.
pub(crate) static INITS: LazyLock<Mutex<BTreeMap<String, Arc<layerfs_api::InitCounters>>>> =
    LazyLock::new(Default::default);
/// This process's live Sandboxes, by their record directory.
#[cfg(unix)]
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
/// [`sandbox_refreshed`].
#[cfg(unix)]
pub(crate) fn sandbox(args: &SdkValue) -> Result<Arc<SandboxState>, SdkRefusal> {
    let directory = crate::discovery::sandbox_dir(args, text(args, "sandbox")?)?;
    let existing = lock(&SANDBOXES)?.get(&directory).cloned();
    match existing {
        Some(state) => Ok(state),
        None => {
            let state = crate::discovery::reopen(args)?;
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
#[cfg(unix)]
pub(crate) fn sandbox_live(args: &SdkValue) -> Result<Arc<SandboxState>, SdkRefusal> {
    let directory = crate::discovery::sandbox_dir(args, text(args, "sandbox")?)?;
    let Some(state) = lock(&SANDBOXES)?.get(&directory).cloned() else {
        return sandbox(args);
    };
    if let Err(failure) = state.runtime.endpoint() {
        lock(&SANDBOXES)?.remove(&directory);
        return Err(if failure.container_gone() || failure.container_stopped() {
            crate::discovery::gone(
                &state.id,
                &state.runtime.identity().to_string(),
                failure.container_stopped(),
            )
        } else {
            crate::job::failed(failure)
        });
    }
    Ok(state)
}

/// `args` naming Sandbox `id` of their project.
#[cfg(unix)]
pub(crate) fn naming(args: &SdkValue, id: &str) -> SdkValue {
    let mut fields = match args {
        SdkValue::Map(fields) => fields
            .iter()
            .filter(|(name, _)| name != "sandbox")
            .cloned()
            .collect(),
        _ => Vec::new(),
    };
    fields.push(("sandbox".to_owned(), SdkValue::text(id)));
    SdkValue::Map(fields)
}

/// The Sandbox an operation names, its tables read again from the shared
/// records and the daemon (one status call per mount).
#[cfg(unix)]
pub(crate) fn sandbox_refreshed(args: &SdkValue) -> Result<Arc<SandboxState>, SdkRefusal> {
    let state = sandbox(args)?;
    crate::discovery::refresh(&state)?;
    Ok(state)
}

#[cfg(unix)]
impl SandboxState {
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
        let known = lock(&self.mounts)?
            .get(path)
            .map(|mount| (mount.workspace.bound.token, pick(mount)));
        match known {
            Some((token, value)) if crate::discovery::workspace_file(self, token).exists() => {
                return Ok(value)
            }
            _ => crate::discovery::refresh(self)?,
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
        .map_err(crate::job::failed)
}
