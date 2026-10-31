//! Launch inputs and observations independent of the daemon control protocol.
use crate::{CommandIdentity, ContainerId};
use std::{fmt, path::PathBuf, time::Duration};
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
    /// The Linux namespace backend: a namespaced daemon plus the Store file it
    /// opens. `store` is the exact file; its directory is what the Sandbox's
    /// private root binds, so SQLite's WAL and the writers' directory lock see
    /// the same inode as every other opener on this kernel. `image` is the
    /// digest its root is an overlay of, and `archive` the docker-archive in
    /// the bundle that digest names, which is unpacked once per digest.
    Namespaces {
        command_identity: CommandIdentity,
        environment: Vec<(String, String)>,
        store: PathBuf,
        image: crate::Image,
        archive: PathBuf,
    },
    Docker {
        image: String,
        /// The Store volume; None mounts none (a PostgreSQL Store, #506).
        store_volume: Option<String>,
        port: u16,
        connect_host: Option<String>,
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
/// Persisted deployment facts for a Docker container; no filesystem discovery here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DockerRecord {
    pub image: String,
    /// None for a PostgreSQL Store's Sandbox, which mounts no volume (#506).
    pub store_volume: Option<String>,
    pub port: u16,
    pub labels: Vec<(String, String)>,
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
    Docker(DockerRecord),
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
    pub fn docker(&self) -> Option<&DockerRecord> {
        match self {
            Self::Docker(record) => Some(record),
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
impl From<DockerRecord> for SandboxRecord {
    fn from(record: DockerRecord) -> Self {
        Self::Docker(record)
    }
}
impl From<&DockerRecord> for SandboxRecord {
    fn from(record: &DockerRecord) -> Self {
        Self::Docker(record.clone())
    }
}
impl From<crate::backend::docker::SandboxRequest> for DockerRecord {
    fn from(request: crate::backend::docker::SandboxRequest) -> Self {
        Self {
            image: request.image,
            store_volume: request.store_volume,
            port: request.port,
            labels: request.labels,
        }
    }
}
impl From<&crate::backend::docker::SandboxRequest> for DockerRecord {
    fn from(request: &crate::backend::docker::SandboxRequest) -> Self {
        Self {
            image: request.image.clone(),
            store_volume: request.store_volume.clone(),
            port: request.port,
            labels: request.labels.clone(),
        }
    }
}
impl From<DockerRecord> for crate::backend::docker::SandboxRequest {
    fn from(record: DockerRecord) -> Self {
        Self {
            image: record.image,
            store_volume: record.store_volume,
            port: record.port,
            labels: record.labels,
        }
    }
}
impl From<crate::backend::docker::SandboxRequest> for SandboxRecord {
    fn from(request: crate::backend::docker::SandboxRequest) -> Self {
        Self::Docker(request.into())
    }
}
impl From<&crate::backend::docker::SandboxRequest> for SandboxRecord {
    fn from(request: &crate::backend::docker::SandboxRequest) -> Self {
        Self::Docker(request.into())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecExit {
    pub code: i32,
    /// Docker may not expose a process id after exit.
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
