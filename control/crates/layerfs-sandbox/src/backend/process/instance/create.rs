//! One process Sandbox start (macOS only): its private directory, its 0600
//! setup, one spawn of the daemon in its own process group and its
//! listener line, the twin of the namespaces backend's
//! `instance/create.rs`. Nothing is retried, and a failure returns what it
//! made so the caller can end and delete it once.
use super::handle::Sandbox;
use crate::spawn::{listener, search_path::search_path, Local, LocalPaths, LocalPhase};
use crate::RuntimeError;
use layerfs_os::process::LocalProcess;
use nix::fcntl::OFlag;
use std::{
    fs::{self, File, OpenOptions, Permissions},
    io::{self, Write},
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

/// A failed create. `sandbox` holds what it made (its directory, and its
/// daemon once spawned), for the caller to stop and delete once.
#[derive(Debug)]
pub struct LocalCreateFailure {
    /// The create phase that failed.
    pub phase: LocalPhase,
    /// What the create made before it failed, for the caller to stop.
    pub sandbox: Option<Sandbox>,
    /// Why it failed.
    pub cause: RuntimeError,
}

impl Local {
    /// Makes the named Sandbox's directory, writes `config` (the encoded
    /// daemon setup naming [`Self::paths`]) 0600, spawns `executable
    /// --config <setup>` in its own process group with no inherited
    /// environment but the search path, and waits up to `wait` for its
    /// listener line. Once anything is made, a failure returns it. A process
    /// daemon's start, macOS only.
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
        let id = crate::ContainerId::random()
            .map_err(|cause| fail(LocalPhase::Validate, None, cause))?;
        // one create of the directory: a name another start holds fails here
        fs::DirBuilder::new()
            .mode(0o755)
            .create(&paths.directory)
            .map_err(|error| fail(LocalPhase::Create, None, error.into()))?;
        let mut sandbox = Sandbox::unstarted(id, paths.directory.clone(), labels);
        let log = match prepare(&paths, config, self.mount_home.as_deref()) {
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
/// returns. With a `mount_home` (#846), its `workspaces` is a link to it,
/// made once: the daemon makes that folder and mounts there, and a stop or
/// delete follows the link to every mount.
fn prepare(paths: &LocalPaths, config: &[u8], mount_home: Option<&Path>) -> io::Result<File> {
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
    if let Some(home) = mount_home {
        std::os::unix::fs::symlink(home, &paths.mounts)?;
    }
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
