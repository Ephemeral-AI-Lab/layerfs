//! The Windows native provider: a Sandbox is its directory tree under
//! the runtime root - no daemon starts (the in-process route runs the
//! Store), commands spawn straight into the directory (A1; the WinFsp
//! mount home is A2). One exact attempt, as on the unix side.
use crate::local::Local as WindowsLocal;
use crate::{
    ContainerId, LaunchFailure, LaunchPhase, ProviderLaunchOptions, RuntimeError,
    SandboxRecord, WireFailure,
};
use std::path::Path;

/// A per-call 64-bit id source: a fresh heap address stirred into the wall
/// clock (the identity needs uniqueness, not cryptographic randomness).
fn rand_id() -> u64 {
    let entropy = Box::into_raw(Box::new(0u8)) as u64;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    entropy.rotate_left(17) ^ now
}

/// Native Windows processes with the caller identity, no daemon.
#[derive(Clone, Debug)]
pub struct HostProcessProvider {
    local: WindowsLocal,
}

impl HostProcessProvider {
    /// Selects `root`; nothing is made.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        Ok(Self {
            local: WindowsLocal::native(root)?,
        })
    }

    /// The named Sandbox paths (its directory tree), made fresh.
    pub fn paths(&self, name: &str) -> Result<crate::local::LocalPaths, RuntimeError> {
        self.local.clone().naming(name)?.paths()
    }

    /// Makes the Sandbox directory tree; no daemon, no listener, no port.
    pub fn launch(&self, request: crate::LaunchRequest) -> Result<crate::runtime::WindowsStub, Box<LaunchFailure>> {
        let fail = |cause: RuntimeError| {
            Box::new(LaunchFailure { phase: LaunchPhase::Create, instance: None, cause })
        };
        let ProviderLaunchOptions::Local { command_identity: _, environment } = request.options else {
            return Err(fail(RuntimeError::Unsupported(
                "native provider requires local launch options",
            )));
        };
        let _ = environment;
        let paths = self.local.clone().naming(&request.name).map_err(fail)?.paths().map_err(fail)?;
        Ok(crate::runtime::WindowsStub {
            id: ContainerId::parse(&format!("{:064x}", rand_id())).map_err(fail)?,
            root: self.root().to_owned(),
            directory: paths.directory,
            labels: request.labels,
            stopped: false,
            deleted: false,
        })
    }

    /// Reopens a recorded Sandbox as its directory (no process identity).
    pub fn reopen(&self, id: ContainerId, record: &SandboxRecord) -> Result<crate::runtime::WindowsStub, Box<WireFailure>> {
        let Some(record) = record.local() else {
            return Err(WireFailure::local(RuntimeError::Unsupported(
                "native provider requires a local record",
            )));
        };
        let paths = self.paths(&record.name).map_err(WireFailure::local)?;
        Ok(crate::runtime::WindowsStub {
            id,
            root: self.root().to_owned(),
            directory: paths.directory,
            labels: record.labels.clone(),
            stopped: true,
            deleted: false,
        })
    }

    /// The runtime root, for the record.
    pub(crate) fn root(&self) -> &Path {
        self.local.root()
    }
}
