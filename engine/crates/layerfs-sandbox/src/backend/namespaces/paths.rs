//! Where one namespace Sandbox keeps its parts: the same private directory a
//! local Sandbox uses, plus the executable it runs and the private root its
//! mount namespace pivots to.
use crate::backend::native::LocalPaths;
use crate::RuntimeError;
use std::path::PathBuf;

/// The daemon executable inside the Sandbox's own `bin` directory, copied
/// there once per Sandbox so nothing about it is read from the host later.
pub(crate) const DAEMON: &str = "layerfs-daemon";

/// One Sandbox's paths, all fixed from its directory name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Paths {
    /// The Sandbox directory with the setup, overlay, mounts, log and `tmp`.
    pub(crate) local: LocalPaths,
    /// `<directory>/bin`, 0755.
    pub(crate) bin_directory: PathBuf,
    /// `<directory>/bin/<DAEMON>`, 0755 and root-owned.
    pub(crate) bin: PathBuf,
    /// The init stage's inputs, `<directory>/config/helper.spec`, 0600.
    pub(crate) spec: PathBuf,
    /// The private root, `<directory>.root`; the init stage mounts a tmpfs on
    /// it and pivots to it, so it is empty again once the namespaces end.
    pub(crate) root: PathBuf,
}

impl Paths {
    /// The paths of the Sandbox named by `local`: its directory name is the
    /// Sandbox's name, and its sibling `<name>.root` is the private root.
    pub(crate) fn of(local: LocalPaths) -> Result<Self, RuntimeError> {
        if (local.directory.file_name())
            .and_then(|name| name.to_str())
            .is_none()
        {
            return Err(RuntimeError::Protocol("namespaces Sandbox directory name"));
        }
        let mut root = local.directory.clone().into_os_string();
        root.push(".root");
        Ok(Self {
            bin_directory: local.directory.join("bin"),
            bin: local.directory.join("bin").join(DAEMON),
            spec: local.directory.join("config").join("helper.spec"),
            root: PathBuf::from(root),
            local,
        })
    }
}
