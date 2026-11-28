//! A local runtime root both backends start daemons under: the Sandbox's
//! private directory and its paths, the start phases and the daemon log.
//! Each backend's start is its own `instance/create.rs` (the process
//! backend's `Local::create`, macOS only).
use crate::RuntimeError;
use std::{
    fs, io,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

/// A local Sandbox's daemon output file in its directory, stdout and
/// stderr together.
pub(crate) const LOG: &str = "daemon.log";

/// A local runtime root: one directory per Sandbox under it.
#[derive(Clone, Debug)]
pub struct Local {
    root: PathBuf,
    name: Option<String>,
    /// Variables the daemon starts with besides `PATH` (a process daemon's,
    /// macOS only).
    #[cfg(target_os = "macos")]
    pub(crate) environment: Vec<(String, String)>,
    /// Where a process daemon mounts its Workspaces when not its own
    /// `workspaces` (#846), macOS only.
    #[cfg(target_os = "macos")]
    pub(crate) mount_home: Option<PathBuf>,
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
    /// Its `workspaces`: a process daemon's link to its mount home (#846:
    /// the home's `sandbox/<name>/workspaces`, made by the daemon). A
    /// namespaces daemon mounts at [`crate::SANDBOX_MOUNTS`] instead.
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

impl Local {
    /// Selects `root`: an existing absolute directory owned by this user,
    /// writable by it alone and traversable by others, as is every directory
    /// above it (commands enter their Workspace below it only after taking
    /// another identity, so one closed directory on the way, such as a 0700
    /// `/root`, would fail every command at spawn).
    pub fn new(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        let local = Self::process(root.as_ref())?;
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
    /// Selects `root` for the process route, whose commands run as this user
    /// (no other identity): the root's own checks as in [`Self::new`], but
    /// the directories above it need not be traversable by others (a macOS
    /// home is 0750).
    pub fn process(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
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
            #[cfg(target_os = "macos")]
            environment: Vec::new(),
            #[cfg(target_os = "macos")]
            mount_home: None,
        })
    }
    /// Mounts the next daemon's Workspaces in `home` (#846): its create
    /// links its own `workspaces` to it. `None` keeps that folder.
    #[cfg(target_os = "macos")]
    pub fn mount_home(mut self, home: Option<PathBuf>) -> Self {
        self.mount_home = home;
        self
    }
    /// Starts the next daemon with `environment` besides its search path (the
    /// kernel route its provider chose and the NFS mount helpers). Nothing
    /// else of this process's environment reaches it.
    #[cfg(target_os = "macos")]
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
}
