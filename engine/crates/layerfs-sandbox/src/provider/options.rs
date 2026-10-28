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
}
/// Persisted physical facts supplied by the record owner; no filesystem discovery here.
#[derive(Clone, Debug)]
pub struct LocalRecord {
    pub name: String,
    pub pid: u32,
    pub start_time: u64,
    pub boot: Option<u128>,
    pub port: u16,
    pub labels: Vec<(String, String)>,
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
