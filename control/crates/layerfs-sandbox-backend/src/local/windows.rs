//! The Windows local Sandbox face: its directory tree, without the unix
//! daemon (the in-process route runs the Store here; a Sandbox is the
//! directory its commands run in, and the WinFsp mount home waits for
//! the A2 batch). One exact attempt, as on the unix side.
use crate::RuntimeError;
use std::{fs, path::{Component, Path, PathBuf}};

/// The same shape as the unix `LocalPaths` (the daemon config and log
/// entries stay so records and tooling read one layout).
pub struct LocalPaths {
    pub directory: PathBuf,
    pub config: PathBuf,
    pub overlay: PathBuf,
    pub mounts: PathBuf,
    pub log: PathBuf,
    pub tmp: PathBuf,
}

/// A local runtime root: one directory per Sandbox under it.
#[derive(Clone, Debug)]
pub struct Local {
    root: PathBuf,
    name: Option<String>,
}

impl Local {
    /// Selects `root`: an existing absolute directory without dot components.
    /// Windows has no uid/mode bits; the home it lives in is private by
    /// construction (ACL-inherited), so the directory itself is the check.
    pub fn native(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        let root = root.as_ref();
        if !root.is_absolute()
            || root
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return Err(RuntimeError::Protocol("absolute local runtime root"));
        }
        fs::symlink_metadata(root)
            .map_err(RuntimeError::Io)?
            .is_dir()
            .then_some(())
            .ok_or(RuntimeError::Protocol("local runtime root must be a directory"))?;
        Ok(Self { root: root.to_owned(), name: None })
    }

    /// Names this Sandbox under the root.
    pub fn naming(mut self, name: &str) -> Result<Self, RuntimeError> {
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains(':') {
            return Err(RuntimeError::Protocol("sandbox name"));
        }
        self.name = Some(name.to_owned());
        Ok(self)
    }

    /// The Sandbox directory tree, made fresh: directory, config, overlay,
    /// mounts, log placeholder and tmp; Windows mode bits do not exist, the
    /// tree inherits the home's privacy.
    pub fn paths(self) -> Result<LocalPaths, RuntimeError> {
        let name = self.name.expect("naming first");
        let directory = self.root.join(&name);
        if directory.exists() {
            return Err(RuntimeError::Protocol("sandbox directory exists"));
        }
        let make = |dir: &Path| fs::create_dir_all(dir).map_err(RuntimeError::Io);
        let config = directory.join("config");
        let overlay = directory.join("overlay");
        let mounts = directory.join("workspaces");
        let tmp = directory.join("tmp");
        for dir in [&directory, &config, &overlay, &mounts, &tmp] {
            make(dir)?;
        }
        let log = directory.join("daemon.log");
        fs::write(&log, b"").map_err(RuntimeError::Io)?;
        Ok(LocalPaths { directory, config, overlay, mounts, log, tmp })
    }
}
