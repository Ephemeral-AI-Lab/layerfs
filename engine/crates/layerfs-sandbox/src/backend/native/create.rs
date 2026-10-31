//! One local daemon start: the Sandbox's private directory, its 0600 setup,
//! one spawn and its listener line. Nothing is retried.
use super::{
    process::LocalProcess,
    sandbox::{Sandbox, LOG},
    search::search_path,
};
use crate::backend::{identity, listener};
use crate::RuntimeError;
use nix::fcntl::OFlag;
use std::{
    fs::{self, File, OpenOptions, Permissions},
    io::{self, Write},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

/// A local runtime root: one directory per Sandbox under it.
#[derive(Clone, Debug)]
pub struct Local {
    root: PathBuf,
    name: Option<String>,
    /// Variables the daemon starts with besides `PATH`.
    environment: Vec<(String, String)>,
}

/// Where one Sandbox keeps its parts, all under its directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalPaths {
    /// The Sandbox directory, `<root>/<name>`, 0755.
    pub directory: PathBuf,
    /// Its daemon setup, `config/daemon.setup`, 0600 in a 0700 directory.
    pub config: PathBuf,
    /// Its daemon overlay template, `overlay/overlay.sqlite`, in a 0700 directory.
    pub overlay: PathBuf,
    /// Its Workspace mount home, `workspaces`, made 0755 by the daemon.
    pub mounts: PathBuf,
    /// Its daemon's output, `daemon.log`, 0600.
    pub log: PathBuf,
    /// Its commands' scratch directory, `tmp`, 1777 like a container's
    /// `/tmp`, so what they leave there goes with the Sandbox.
    pub tmp: PathBuf,
}

/// The boundary one failed create reached.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalPhase {
    /// Local checks; nothing made.
    Validate,
    /// The directory, setup and log file.
    Create,
    /// The daemon spawn.
    Start,
    /// The wait for the daemon's listener line.
    Listener,
}

/// A failed create. `sandbox` holds what it made (its directory, and its
/// daemon once spawned), for the caller to stop and delete once.
#[derive(Debug)]
pub struct LocalCreateFailure {
    pub phase: LocalPhase,
    pub sandbox: Option<Sandbox>,
    pub cause: RuntimeError,
}

impl Local {
    /// Selects `root`: an existing absolute directory owned by this user,
    /// writable by it alone and traversable by others, as is every directory
    /// above it (commands enter their Workspace below it only after taking
    /// another identity, so one closed directory on the way, such as a 0700
    /// `/root`, would fail every command at spawn).
    pub fn new(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        let local = Self::native(root.as_ref())?;
        let root = root.as_ref();
        // every directory on the path as given and on the path it resolves to
        let resolved = fs::canonicalize(root)?;
        for directory in root.ancestors().chain(resolved.ancestors()) {
            let metadata = fs::metadata(directory)?;
            if !metadata.is_dir() || metadata.mode() & 0o001 == 0 {
                return Err(RuntimeError::Io(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "{} is not traversable by others (o+x); local Sandbox commands run as \
                         another identity and must pass it to reach their Workspace",
                        directory.display()
                    ),
                )));
            }
        }
        Ok(local)
    }
    /// Selects `root` for the native route, whose commands run as this user
    /// (no other identity): the root's own checks as in [`Self::new`], but
    /// the directories above it need not be traversable by others (a macOS
    /// home is 0750).
    pub fn native(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        let root = root.as_ref();
        if !root.is_absolute()
            || root
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return Err(RuntimeError::Protocol("absolute local runtime root"));
        }
        let metadata = fs::symlink_metadata(root)?;
        if !metadata.is_dir()
            || metadata.uid() != nix::unistd::Uid::effective().as_raw()
            || metadata.mode() & 0o022 != 0
            || metadata.mode() & 0o001 == 0
        {
            return Err(RuntimeError::Protocol(
                "local runtime root must be a directory of this user, not group/other writable, traversable by others (0755)",
            ));
        }
        Ok(Self {
            root: root.to_owned(),
            name: None,
            environment: Vec::new(),
        })
    }
    /// Starts the next daemon with `environment` besides its search path (the
    /// native route's `LAYERFS_KERNEL=nfs` and its mount helpers). Nothing
    /// else of this process's environment reaches it.
    pub fn environment(mut self, environment: Vec<(String, String)>) -> Self {
        self.environment = environment;
        self
    }
    /// Names the next Sandbox's directory `name`, under the Docker name rule
    /// (an ASCII letter or digit, then letters, digits, `_`, `.`, `-`, at
    /// most 128), so it is one plain path component.
    pub fn naming(mut self, name: &str) -> Result<Self, RuntimeError> {
        let bytes = name.as_bytes();
        if bytes.is_empty()
            || bytes.len() > 128
            || !bytes[0].is_ascii_alphanumeric()
            || !bytes
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
        {
            return Err(RuntimeError::Protocol("local Sandbox name"));
        }
        self.name = Some(name.to_owned());
        Ok(self)
    }
    /// The named Sandbox's paths, for its daemon setup.
    pub fn paths(&self) -> Result<LocalPaths, RuntimeError> {
        let name = self
            .name
            .as_deref()
            .ok_or(RuntimeError::Protocol("local Sandbox name"))?;
        let directory = self.root.join(name);
        Ok(LocalPaths {
            config: directory.join("config").join("daemon.setup"),
            overlay: directory.join("overlay").join("overlay.sqlite"),
            mounts: directory.join("workspaces"),
            log: directory.join(LOG),
            tmp: directory.join("tmp"),
            directory,
        })
    }
    /// Makes the named Sandbox's directory, writes `config` (the encoded
    /// daemon setup naming [`Self::paths`]) 0600, spawns `executable
    /// --config <setup>` in its own process group with no inherited
    /// environment but the search path, and waits up to `wait` for its
    /// listener line. Once anything is made, a failure returns it.
    pub fn create(
        &self,
        executable: &Path,
        config: &[u8],
        wait: Duration,
        labels: Vec<(String, String)>,
    ) -> Result<Sandbox, Box<LocalCreateFailure>> {
        let fail = |phase, sandbox, cause| {
            Box::new(LocalCreateFailure {
                phase,
                sandbox,
                cause,
            })
        };
        let paths = self
            .paths()
            .map_err(|cause| fail(LocalPhase::Validate, None, cause))?;
        let checked = (|| {
            let metadata = fs::metadata(executable)?;
            if !executable.is_absolute() || !metadata.is_file() || metadata.len() == 0 {
                return Err(RuntimeError::Protocol("local daemon executable"));
            }
            if config.is_empty() || config.len() > 8192 || wait.is_zero() {
                return Err(RuntimeError::Protocol("local daemon setup/startup wait"));
            }
            match fs::symlink_metadata(&paths.directory) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.into()),
                Ok(_) => Err(RuntimeError::Protocol("local Sandbox directory exists")),
            }
        })();
        checked.map_err(|cause| fail(LocalPhase::Validate, None, cause))?;
        let id = identity::random().map_err(|cause| fail(LocalPhase::Validate, None, cause))?;
        // one create of the directory: a name another start holds fails here
        fs::DirBuilder::new()
            .mode(0o755)
            .create(&paths.directory)
            .map_err(|error| fail(LocalPhase::Create, None, error.into()))?;
        let mut sandbox = Sandbox::unstarted(id, paths.directory.clone(), labels);
        let log = match prepare(&paths, config) {
            Ok(log) => log,
            Err(error) => return Err(fail(LocalPhase::Create, Some(sandbox), error.into())),
        };
        let spawned = log.try_clone().and_then(|output| {
            Command::new(executable)
                .arg("--config")
                .arg(&paths.config)
                .env_clear()
                .env("PATH", search_path())
                .envs(self.environment.iter().map(|(k, v)| (k, v)))
                .current_dir(&paths.directory)
                .stdin(Stdio::null())
                .stdout(output)
                .stderr(log)
                .process_group(0)
                .spawn()
        });
        let mut child = match spawned {
            Ok(child) => child,
            Err(error) => return Err(fail(LocalPhase::Start, Some(sandbox), error.into())),
        };
        let process = match LocalProcess::observe(child.id()) {
            Ok(process) => process,
            Err(error) => {
                // spawned but unobservable: end it here, it has no identity to record
                let _ = child.kill();
                let _ = child.wait();
                return Err(fail(LocalPhase::Start, Some(sandbox), error.into()));
            }
        };
        sandbox.process = Some(process);
        *sandbox
            .child
            .get_mut()
            .unwrap_or_else(|error| error.into_inner()) = Some(child);
        match listener::await_listener(&paths.log, wait, &|| {
            sandbox.reap();
            sandbox.process.is_some_and(LocalProcess::alive)
        }) {
            Ok(port) => {
                sandbox.port = port;
                Ok(sandbox)
            }
            Err(cause) => Err(fail(LocalPhase::Listener, Some(sandbox), cause)),
        }
    }
}

/// The setup and overlay directories (0700), the scratch directory (1777),
/// the setup file (0600, written once) and the empty log file (0600) it
/// returns.
fn prepare(paths: &LocalPaths, config: &[u8]) -> io::Result<File> {
    // the process umask may narrow the create mode
    fs::set_permissions(&paths.directory, Permissions::from_mode(0o755))?;
    for file in [&paths.config, &paths.overlay] {
        let parent = file
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "local path"))?;
        fs::DirBuilder::new().mode(0o700).create(parent)?;
    }
    fs::DirBuilder::new().mode(0o700).create(&paths.tmp)?;
    fs::set_permissions(&paths.tmp, Permissions::from_mode(0o1777))?;
    let private = |path: &Path| {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC).bits())
            .open(path)
    };
    private(&paths.config)?.write_all(config)?;
    private(&paths.log)
}
