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
    Docker {
        image: String,
        store_volume: String,
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
/// Persisted deployment facts for a Docker container; no filesystem discovery here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DockerRecord {
    pub image: String,
    pub store_volume: String,
    pub port: u16,
    pub labels: Vec<(String, String)>,
}
/// Recorded runtime identity and deployment facts for reopen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SandboxRecord {
    Local(LocalRecord),
    Docker(DockerRecord),
}
impl SandboxRecord {
    pub fn local(&self) -> Option<&LocalRecord> {
        match self {
            Self::Local(record) => Some(record),
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
