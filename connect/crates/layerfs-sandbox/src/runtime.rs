//! The concrete runtimes a Sandbox can run on: one enum names the deployment's
//! choice, and [`Sandbox`] is the launched owner over it. What is fixed per
//! backend (capabilities) comes from its `BackendAdapter` (`backend::adapter`);
//! each provider and Sandbox is a concrete type, matched here.
use crate::backend::docker::{self, DockerProvider};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use crate::backend::host_process;
#[cfg(target_os = "linux")]
use crate::backend::namespaces::{self, NamespaceProvider};
use crate::backend::{adapter, Backend};
#[cfg(target_os = "macos")]
use crate::host_process::HostProcessProvider;
use crate::{
    Bind, ExecExit, LaunchPhase, LaunchRequest, LocalRecord, ProviderCapabilities,
    SandboxInspection, SandboxRecord,
};
use crate::{ContainerId, ExecRequest, RuntimeError, WireFailure};
use std::{fmt, io::Write, net::SocketAddr, path::PathBuf};

/// One deployment's runtime choice, kept for every Sandbox it starts.
///
/// A Sandbox's record names the variant that made it; a platform without a
/// runtime is refused before any effect, never replaced by another backend.
#[derive(Clone, Debug)]
pub enum Runtime {
    /// A local Docker Engine container; the default.
    Docker(DockerProvider),
    /// A local daemon process serving loopback NFS as this user (macOS;
    /// Linux has no native runtime, #533 O27).
    #[cfg(target_os = "macos")]
    HostProcess(HostProcessProvider),
    /// A root daemon in its own mount, PID and IPC namespaces and cgroup,
    /// commands entering them as a nonroot identity (Linux).
    #[cfg(target_os = "linux")]
    Namespaces(NamespaceProvider),
}

impl Runtime {
    /// The backend this runtime is.
    pub fn backend(&self) -> Backend {
        match self {
            Self::Docker(_) => Backend::Docker,
            #[cfg(target_os = "macos")]
            Self::HostProcess(_) => Backend::HostProcess,
            #[cfg(target_os = "linux")]
            Self::Namespaces(_) => Backend::Namespaces,
        }
    }

    pub fn capabilities(&self) -> Result<ProviderCapabilities, RuntimeError> {
        Ok(adapter(self.backend()).capabilities())
    }

    /// Local resource ownership, checked by API before encoding daemon topology.
    /// Nonlocal providers return None. No resource is created by this observation.
    pub fn local_directory(&self, name: &str) -> Result<Option<PathBuf>, RuntimeError> {
        match self {
            Self::Docker(_) => Ok(None),
            #[cfg(target_os = "macos")]
            Self::HostProcess(provider) => provider.paths(name).map(|paths| Some(paths.directory)),
            #[cfg(target_os = "linux")]
            Self::Namespaces(provider) => provider.paths(name).map(|paths| Some(paths.directory)),
        }
    }

    /// The named native Sandbox's paths, for its daemon setup.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn native_paths(&self, name: &str) -> Result<crate::local::LocalPaths, RuntimeError> {
        match self {
            #[cfg(target_os = "macos")]
            Self::HostProcess(provider) => provider.paths(name),
            #[cfg(target_os = "linux")]
            Self::Namespaces(provider) => provider.paths(name),
            Self::Docker(_) => Err(RuntimeError::Unsupported(
                "native Sandbox paths need a native runtime",
            )),
        }
    }

    /// Starts one Sandbox once; a failed launch keeps the acknowledged owner.
    pub fn launch(&self, request: LaunchRequest) -> Result<Sandbox, Box<LaunchFailure>> {
        match self {
            Self::Docker(provider) => provider.launch(request).map(Sandbox::Docker),
            #[cfg(target_os = "macos")]
            Self::HostProcess(provider) => provider.launch(request).map(Sandbox::HostProcess),
            #[cfg(target_os = "linux")]
            Self::Namespaces(provider) => provider
                .launch(request)
                .map(|sandbox| Sandbox::Namespaces(Box::new(sandbox))),
        }
    }

    /// Starts one Sandbox once with exactly `binds` in its private root
    /// (#535 §6.3: a sqlite Store's [`Bind::store`]; none for a PostgreSQL
    /// Store). Only the namespaces backend binds: its options' `store` must
    /// then be empty. Docker takes its Store as a volume and a host-process
    /// daemon shares the host's files, so they refuse a bind before any
    /// effect. [`Self::launch`] is the same with the namespaces options'
    /// `store` as the one bind.
    pub fn launch_with_binds(
        &self,
        request: LaunchRequest,
        binds: Vec<Bind>,
    ) -> Result<Sandbox, Box<LaunchFailure>> {
        let unbound = |why| {
            Box::new(LaunchFailure {
                phase: LaunchPhase::Validate,
                instance: None,
                cause: RuntimeError::Unsupported(why),
            })
        };
        match self {
            Self::Docker(_) if !binds.is_empty() => Err(unbound(
                "a docker Sandbox takes its Store as a volume, not a bind",
            )),
            #[cfg(target_os = "macos")]
            Self::HostProcess(_) if !binds.is_empty() => Err(unbound(
                "a host-process Sandbox shares the host's files and takes no bind",
            )),
            #[cfg(target_os = "linux")]
            Self::Namespaces(provider) => provider
                .launch_with_binds(request, binds)
                .map(|sandbox| Sandbox::Namespaces(Box::new(sandbox))),
            _ => self.launch(request),
        }
    }

    /// Reopen starts nothing and checks the recorded deployment or process identity.
    pub fn reopen(
        &self,
        id: ContainerId,
        record: &SandboxRecord,
    ) -> Result<Sandbox, Box<WireFailure>> {
        match self {
            Self::Docker(provider) => provider.reopen(id, record).map(Sandbox::Docker),
            #[cfg(target_os = "macos")]
            Self::HostProcess(provider) => provider.reopen(id, record).map(Sandbox::HostProcess),
            #[cfg(target_os = "linux")]
            Self::Namespaces(provider) => provider
                .reopen(id, record)
                .map(|sandbox| Sandbox::Namespaces(Box::new(sandbox))),
        }
    }

    /// A known stopped record admits only the existing runtime's single Delete.
    pub fn reopen_stopped(
        &self,
        id: ContainerId,
        record: &SandboxRecord,
    ) -> Result<Sandbox, Box<WireFailure>> {
        match self {
            Self::Docker(provider) => provider.reopen_stopped(id, record).map(Sandbox::Docker),
            #[cfg(target_os = "macos")]
            Self::HostProcess(provider) => provider
                .reopen_stopped(id, record)
                .map(Sandbox::HostProcess),
            #[cfg(target_os = "linux")]
            Self::Namespaces(provider) => provider
                .reopen_stopped(id, record)
                .map(|sandbox| Sandbox::Namespaces(Box::new(sandbox))),
        }
    }
}

/// The launched owner of one Sandbox, on the runtime that started it.
#[derive(Debug)]
pub enum Sandbox {
    Docker(docker::Sandbox),
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    HostProcess(host_process::Sandbox),
    #[cfg(target_os = "linux")]
    Namespaces(Box<namespaces::Sandbox>),
}

impl Sandbox {
    /// The backend that launched it.
    pub fn backend(&self) -> Backend {
        match self {
            Self::Docker(_) => Backend::Docker,
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::HostProcess(_) => Backend::HostProcess,
            #[cfg(target_os = "linux")]
            Self::Namespaces(_) => Backend::Namespaces,
        }
    }

    pub fn identity(&self) -> ContainerId {
        match self {
            Self::Docker(sandbox) => sandbox.identity(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::HostProcess(sandbox) => sandbox.identity(),
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.identity(),
        }
    }

    pub fn endpoint(&self) -> Result<SocketAddr, Box<WireFailure>> {
        match self {
            Self::Docker(sandbox) => sandbox.endpoint(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::HostProcess(sandbox) => sandbox.endpoint(),
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.endpoint(),
        }
    }

    /// One command to its end, its streams copied to the sinks as they arrive.
    pub fn run(
        &self,
        request: &ExecRequest,
        stdout: &mut (dyn Write + Send),
        stderr: &mut (dyn Write + Send),
    ) -> Result<ExecExit, Box<WireFailure>> {
        match self {
            Self::Docker(sandbox) => docker::run_command(sandbox, request, stdout, stderr),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::HostProcess(sandbox) => sandbox.run(request, stdout, stderr),
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.run(request, stdout, stderr),
        }
    }

    /// Names a background process a command started in a session of its own,
    /// so it ends with the Sandbox as a container's descendants do.
    pub fn track(&self, pid: u32, start_time: u64) -> Result<(), Box<WireFailure>> {
        match self {
            // Container teardown owns all descendants; no host process is adopted.
            Self::Docker(_) => Ok(()),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::HostProcess(sandbox) => sandbox.track(pid, start_time),
            // its cgroup owns every process the Sandbox started, whatever
            // session it left, so nothing needs naming
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.track(pid, start_time),
        }
    }

    pub fn log_tail(&self, lines: u16, cap: usize) -> Result<Vec<u8>, Box<WireFailure>> {
        match self {
            Self::Docker(sandbox) => sandbox.log_tail(lines, cap),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::HostProcess(sandbox) => sandbox.log_tail(lines, cap),
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.log_tail(lines, cap),
        }
    }

    pub fn inspect(&self) -> Result<SandboxInspection, Box<WireFailure>> {
        match self {
            Self::Docker(sandbox) => sandbox.inspect(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::HostProcess(sandbox) => {
                sandbox.reap();
                Ok(SandboxInspection {
                    identity: sandbox.identity(),
                    running: sandbox.process().is_some_and(|p| p.alive()),
                    exit_code: None,
                    process_id: sandbox.process().map(|p| p.pid),
                    scratch_bytes: None,
                })
            }
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => {
                sandbox.reap();
                Ok(SandboxInspection {
                    identity: sandbox.identity(),
                    running: sandbox.process().is_some_and(|p| p.alive()),
                    exit_code: None,
                    process_id: sandbox.process().map(|p| p.pid),
                    scratch_bytes: None,
                })
            }
        }
    }

    pub fn stop(&mut self, timeout_seconds: u16) -> Result<(), Box<WireFailure>> {
        match self {
            Self::Docker(sandbox) => sandbox.stop(timeout_seconds),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::HostProcess(sandbox) => sandbox.stop(timeout_seconds),
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.stop(timeout_seconds),
        }
    }

    pub fn delete(&mut self) -> Result<(), Box<WireFailure>> {
        match self {
            Self::Docker(sandbox) => sandbox.delete(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::HostProcess(sandbox) => sandbox.delete(),
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.delete(),
        }
    }

    /// Where its commands keep scratch files: `/tmp` in a container, the
    /// Sandbox's own directory for a native one, deleted with it.
    pub fn scratch_directory(&self) -> String {
        match self {
            Self::Docker(_) => "/tmp".into(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::HostProcess(sandbox) => sandbox.tmp().display().to_string(),
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.tmp().display().to_string(),
        }
    }

    /// Physical facts for the binding's existing record format, when this is native.
    pub fn local_record(&self) -> Option<LocalRecord> {
        match self {
            Self::Docker(_) => None,
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::HostProcess(sandbox) => sandbox.local_record(),
            #[cfg(target_os = "linux")]
            Self::Namespaces(_) => None,
        }
    }

    /// Persisted deployment facts for reopening this runtime instance.
    pub fn record(&self) -> Option<SandboxRecord> {
        match self {
            Self::Docker(sandbox) => Some(SandboxRecord::Docker(sandbox.request().clone().into())),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::HostProcess(_) => self.local_record().map(SandboxRecord::Local),
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.record(),
        }
    }
}

/// The caller retains the exact resource even when launch fails after creating it.
pub struct LaunchFailure {
    pub phase: LaunchPhase,
    pub instance: Option<Box<Sandbox>>,
    pub cause: RuntimeError,
}
impl fmt::Debug for LaunchFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchFailure")
            .field("phase", &self.phase)
            .field("identity", &self.instance.as_ref().map(|i| i.identity()))
            .field("cause", &self.cause)
            .finish()
    }
}
