//! Launch inputs and observations independent of the daemon control protocol.
use crate::{CommandIdentity, ContainerId};
use std::{fmt, path::PathBuf, time::Duration};
/// The last launch phase a provider reached.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchPhase {
    Validate,
    Create,
    Start,
    Listener,
}
/// Configuration bytes may contain secrets; Debug only reports their length.
#[derive(Clone)]
pub struct LaunchRequest {
    pub name: String,
    pub daemon_executable: PathBuf,
    pub daemon_config: Vec<u8>,
    pub labels: Vec<(String, String)>,
    pub startup_timeout: Duration,
    pub options: ProviderLaunchOptions,
}
impl fmt::Debug for LaunchRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchRequest")
            .field("name", &self.name)
            .field("daemon_executable", &self.daemon_executable)
            .field("config_bytes", &self.daemon_config.len())
            .field("startup_timeout", &self.startup_timeout)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug)]
pub enum ProviderLaunchOptions {
    Local {
        command_identity: CommandIdentity,
        environment: Vec<(String, String)>,
    },
    /// The Linux namespace backend: a namespaced daemon whose private root
    /// takes the launch's binds (a sqlite Store's [`crate::Bind::store`],
    /// none for a PostgreSQL Store). `image` is the digest its root is an
    /// overlay of, and `archive` the docker-archive in the bundle that
    /// digest names, which is unpacked once per digest.
    Namespaces {
        command_identity: CommandIdentity,
        environment: Vec<(String, String)>,
        image: crate::Image,
        archive: PathBuf,
    },
}
/// Persisted physical facts supplied by the record owner; no filesystem discovery here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalRecord {
    pub name: String,
    pub pid: u32,
    pub start_time: u64,
    pub boot: Option<u128>,
    pub port: u16,
    pub labels: Vec<(String, String)>,
}
/// A namespaced Linux Sandbox's record: the local facts (its directory name,
/// init process identity and listener port) and the image digest its root is
/// an overlay of, so a reopen finds the same unpacked image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamespaceRecord {
    pub local: LocalRecord,
    pub image: crate::Image,
}
impl NamespaceRecord {
    /// The physical local facts every record of this backend keeps.
    pub fn local(&self) -> &LocalRecord {
        &self.local
    }
    /// The image digest the Sandbox's root is an overlay of.
    pub fn image(&self) -> &crate::Image {
        &self.image
    }
}
/// Recorded runtime identity and deployment facts for reopen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SandboxRecord {
    Local(LocalRecord),
    /// A namespaced Linux Sandbox; the physical facts kept are the same as a
    /// local one (its directory name, init process identity and listener port),
    /// plus the image digest its root is an overlay of. The variant itself
    /// names the backend, so a record is never reopened by another one.
    Namespaces(NamespaceRecord),
}
impl SandboxRecord {
    pub fn local(&self) -> Option<&LocalRecord> {
        match self {
            Self::Local(record) => Some(record),
            _ => None,
        }
    }
    pub fn namespaces(&self) -> Option<&NamespaceRecord> {
        match self {
            Self::Namespaces(record) => Some(record),
            _ => None,
        }
    }
}
impl From<LocalRecord> for SandboxRecord {
    fn from(record: LocalRecord) -> Self {
        Self::Local(record)
    }
}
impl From<&LocalRecord> for SandboxRecord {
    fn from(record: &LocalRecord) -> Self {
        Self::Local(record.clone())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecExit {
    pub code: i32,
    /// None when no process id is known after exit.
    pub pid: Option<u32>,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
}
/// Unknown resource/timestamp fields stay None rather than invented zeroes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxInspection {
    pub identity: ContainerId,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub process_id: Option<u32>,
    pub scratch_bytes: Option<u64>,
}
