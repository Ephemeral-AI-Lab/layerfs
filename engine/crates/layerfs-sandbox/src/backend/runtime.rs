//! The concrete runtimes a Sandbox can run on: one enum names the deployment's
//! choice, and [`Sandbox`] is the launched owner over it. There is no provider
//! trait; each backend is a concrete type and dispatch is a `match` here.
use super::docker::{self, DockerProvider};
#[cfg(target_os = "linux")]
use super::namespaces::{self, NamespaceProvider};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use super::native::{self, NativeLinuxProvider, NativeMacosProvider};
use crate::provider::{
    ExecExit, LaunchFailure, LaunchRequest, LocalRecord, ProviderCapabilities, SandboxInspection,
    SandboxRecord,
};
use crate::{ContainerId, ExecRequest, RuntimeError, WireFailure};
use std::{io::Write, net::SocketAddr, path::PathBuf};

/// What Windows would need and does not have yet: no host runtime exists.
const WINDOWS: &str = "windows native Sandbox runtime";

/// One deployment's runtime choice, kept for every Sandbox it starts.
///
/// A Sandbox's record names the variant that made it; a platform without a
/// runtime is refused before any effect, never replaced by another backend.
#[derive(Clone, Debug)]
pub enum Runtime {
    /// A local Docker Engine container; the default.
    Docker(DockerProvider),
    /// A local daemon process serving loopback NFS as this user (macOS).
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    MacosNative(NativeMacosProvider),
    /// A root local daemon process serving FUSE, commands as a nonroot
    /// identity (Linux).
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    LinuxNative(NativeLinuxProvider),
    /// A root daemon in its own mount, PID and IPC namespaces and cgroup,
    /// commands entering them as a nonroot identity (Linux).
    #[cfg(target_os = "linux")]
    Namespaces(NamespaceProvider),
    /// Placeholder: selecting Windows refuses before any effect.
    Windows,
}

impl Runtime {
    pub fn capabilities(&self) -> Result<ProviderCapabilities, RuntimeError> {
        match self {
            Self::Docker(provider) => Ok(provider.capabilities()),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::MacosNative(provider) => Ok(provider.capabilities()),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::LinuxNative(provider) => Ok(provider.capabilities()),
            #[cfg(target_os = "linux")]
            Self::Namespaces(provider) => Ok(provider.capabilities()),
            Self::Windows => Err(RuntimeError::Unsupported(WINDOWS)),
        }
    }

    /// Local resource ownership, checked by API before encoding daemon topology.
    /// Nonlocal providers return None. No resource is created by this observation.
    pub fn local_directory(&self, name: &str) -> Result<Option<PathBuf>, RuntimeError> {
        match self {
            Self::Docker(_) => Ok(None),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::MacosNative(provider) => provider.paths(name).map(|paths| Some(paths.directory)),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::LinuxNative(provider) => provider.paths(name).map(|paths| Some(paths.directory)),
            #[cfg(target_os = "linux")]
            Self::Namespaces(provider) => provider.paths(name).map(|paths| Some(paths.directory)),
            Self::Windows => Err(RuntimeError::Unsupported(WINDOWS)),
        }
    }

    /// The named native Sandbox's paths, for its daemon setup.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn native_paths(&self, name: &str) -> Result<native::LocalPaths, RuntimeError> {
        match self {
            Self::MacosNative(provider) => provider.paths(name),
            Self::LinuxNative(provider) => provider.paths(name),
            #[cfg(target_os = "linux")]
            Self::Namespaces(provider) => provider.paths(name),
            Self::Docker(_) | Self::Windows => Err(RuntimeError::Unsupported(
                "native Sandbox paths need a native runtime",
            )),
        }
    }

    /// Starts one Sandbox once; a failed launch keeps the acknowledged owner.
    pub fn launch(&self, request: LaunchRequest) -> Result<Sandbox, Box<LaunchFailure>> {
        match self {
            Self::Docker(provider) => provider.launch(request).map(Sandbox::Docker),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::MacosNative(provider) => provider.launch(request).map(Sandbox::Native),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::LinuxNative(provider) => provider.launch(request).map(Sandbox::Native),
            #[cfg(target_os = "linux")]
            Self::Namespaces(provider) => provider
                .launch(request)
                .map(|sandbox| Sandbox::Namespaces(Box::new(sandbox))),
            Self::Windows => Err(Box::new(LaunchFailure {
                phase: crate::provider::LaunchPhase::Validate,
                instance: None,
                cause: RuntimeError::Unsupported(WINDOWS),
            })),
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
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::MacosNative(provider) => provider.reopen(id, record).map(Sandbox::Native),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::LinuxNative(provider) => provider.reopen(id, record).map(Sandbox::Native),
            #[cfg(target_os = "linux")]
            Self::Namespaces(provider) => provider
                .reopen(id, record)
                .map(|sandbox| Sandbox::Namespaces(Box::new(sandbox))),
            Self::Windows => Err(WireFailure::local(RuntimeError::Unsupported(WINDOWS))),
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
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::MacosNative(provider) => provider.reopen_stopped(id, record).map(Sandbox::Native),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::LinuxNative(provider) => provider.reopen_stopped(id, record).map(Sandbox::Native),
            #[cfg(target_os = "linux")]
            Self::Namespaces(provider) => provider
                .reopen_stopped(id, record)
                .map(|sandbox| Sandbox::Namespaces(Box::new(sandbox))),
            Self::Windows => Err(WireFailure::local(RuntimeError::Unsupported(WINDOWS))),
        }
    }
}

/// The launched owner of one Sandbox, on the runtime that started it.
#[derive(Debug)]
pub enum Sandbox {
    Docker(docker::Sandbox),
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Native(native::Sandbox),
    #[cfg(target_os = "linux")]
    Namespaces(Box<namespaces::Sandbox>),
}

impl Sandbox {
    pub fn identity(&self) -> ContainerId {
        match self {
            Self::Docker(sandbox) => sandbox.identity(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Native(sandbox) => sandbox.identity(),
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.identity(),
        }
    }

    pub fn endpoint(&self) -> Result<SocketAddr, Box<WireFailure>> {
        match self {
            Self::Docker(sandbox) => sandbox.endpoint(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Native(sandbox) => sandbox.endpoint(),
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
            Self::Native(sandbox) => {
                let exit = sandbox.run(request, stdout, stderr)?;
                Ok(ExecExit {
                    code: exit.code,
                    pid: Some(exit.pid),
                    stdout_bytes: exit.stdout,
                    stderr_bytes: exit.stderr,
                })
            }
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
            Self::Native(sandbox) => sandbox.track(pid, start_time),
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
            Self::Native(sandbox) => sandbox.log_tail(lines, cap),
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.log_tail(lines, cap),
        }
    }

    pub fn inspect(&self) -> Result<SandboxInspection, Box<WireFailure>> {
        match self {
            Self::Docker(sandbox) => sandbox.inspect(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Native(sandbox) => {
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
            Self::Native(sandbox) => sandbox.stop(timeout_seconds),
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.stop(timeout_seconds),
        }
    }

    pub fn delete(&mut self) -> Result<(), Box<WireFailure>> {
        match self {
            Self::Docker(sandbox) => sandbox.delete(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Native(sandbox) => sandbox.delete(),
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
            Self::Native(sandbox) => sandbox.tmp().display().to_string(),
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.tmp().display().to_string(),
        }
    }

    /// Physical facts for the binding's existing record format, when this is native.
    pub fn local_record(&self) -> Option<LocalRecord> {
        match self {
            Self::Docker(_) => None,
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Native(sandbox) => sandbox.local_record(),
            #[cfg(target_os = "linux")]
            Self::Namespaces(_) => None,
        }
    }

    /// Persisted deployment facts for reopening this runtime instance.
    pub fn record(&self) -> Option<SandboxRecord> {
        match self {
            Self::Docker(sandbox) => Some(SandboxRecord::Docker(sandbox.request().clone().into())),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Native(_) => self.local_record().map(SandboxRecord::Local),
            #[cfg(target_os = "linux")]
            Self::Namespaces(sandbox) => sandbox.record(),
        }
    }
}
