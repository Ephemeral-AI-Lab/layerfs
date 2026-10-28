//! Docker's existing one-attempt runtime behind the shared provider ports.
use super::{Docker, Sandbox, SandboxRequest};
use crate::provider::{
    ExecExit, IsolationModel, LaunchFailure, LaunchPhase, LaunchRequest, LocalRecord, MountDriver,
    ProviderCapabilities, ProviderLaunchOptions, SandboxInspection, SandboxInstance,
    SandboxProvider, SearchPathPolicy,
};
use crate::{ContainerId, ExecRequest, RuntimeError, WireFailure};
use std::{fs::File, io::Write, net::SocketAddr};

/// Negotiation and all transfers use the supplied client's original policy and version cache.
#[derive(Clone, Debug)]
pub struct DockerProvider {
    docker: Docker,
    deployment: SandboxRequest,
}
/// The original container owner, including its exact attempted/acknowledged lifecycle state.
pub type DockerInstance = Sandbox;
impl DockerProvider {
    /// Deployment labels must match the launch labels; the record format is unchanged.
    pub fn new(docker: Docker, deployment: SandboxRequest) -> Self {
        Self { docker, deployment }
    }
    /// Reads the existing exact deployment; starts, installs and replays nothing.
    pub fn reopen_docker(
        &self,
        id: ContainerId,
        request: SandboxRequest,
    ) -> Result<Box<dyn SandboxInstance>, Box<WireFailure>> {
        Sandbox::reopen(self.docker.clone(), id, request)
            .map(|s| Box::new(s) as Box<dyn SandboxInstance>)
    }
    /// Caller must already hold an exact stopped observation. Delete remains non-forcing.
    pub fn reopen_stopped_docker(
        &self,
        id: ContainerId,
        request: SandboxRequest,
    ) -> Result<Box<dyn SandboxInstance>, Box<WireFailure>> {
        request.check().map_err(WireFailure::local)?;
        Ok(Box::new(Sandbox::stopped(self.docker.clone(), id, request)))
    }
}
impl SandboxProvider for DockerProvider {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            name: "docker",
            isolation: IsolationModel::Container,
            mount_driver: MountDriver::Fuse,
            requires_image: true,
            search_path_policy: SearchPathPolicy::FixedContainer,
            supports_resource_limits: false,
            direct_store_access: true,
        }
    }
    fn local_directory(&self, _: &str) -> Result<Option<std::path::PathBuf>, RuntimeError> {
        Ok(None)
    }
    fn launch(
        &self,
        request: LaunchRequest,
    ) -> Result<Box<dyn SandboxInstance>, Box<LaunchFailure>> {
        let mut phase = LaunchPhase::Validate;
        let mut instance = None;
        let result = (|| {
            let ProviderLaunchOptions::Local {
                command_identity,
                environment,
            } = request.options;
            command_identity.check()?;
            if !environment.is_empty() || request.labels != self.deployment.labels {
                return Err(RuntimeError::Protocol("Docker launch labels/environment"));
            }
            let deployment = self.deployment.clone();
            deployment.check()?;
            if request.startup_timeout.is_zero()
                || request.daemon_config.is_empty()
                || request.daemon_config.len() > 8192
            {
                return Err(RuntimeError::Protocol("Docker startup wait/private config"));
            }
            let docker = self.docker.clone().naming(&request.name)?;
            // Opening the executable precedes any Engine effect, as in the original API route.
            let mut file = File::open(request.daemon_executable)?;
            phase = LaunchPhase::Create;
            instance =
                Some(docker.create_sandbox(deployment).map_err(|f| {
                    RuntimeError::Docker(Box::new(super::ProviderFailure::Create(f)))
                })?);
            let owner = instance.as_mut().expect("acknowledged container");
            owner
                .upload(&mut file, &request.daemon_config)
                .map_err(|f| RuntimeError::Docker(Box::new(super::ProviderFailure::Upload(f))))?;
            phase = LaunchPhase::Start;
            owner
                .start()
                .map_err(|f| RuntimeError::Docker(Box::new(super::ProviderFailure::Start(f))))?;
            phase = LaunchPhase::Listener;
            owner
                .wait_listener(request.startup_timeout)
                .map_err(|f| RuntimeError::Docker(Box::new(super::ProviderFailure::Listener(f))))?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(Box::new(instance.expect("launched"))),
            Err(cause) => Err(Box::new(LaunchFailure {
                phase,
                instance: instance.map(|s| Box::new(s) as Box<dyn SandboxInstance>),
                cause,
            })),
        }
    }
    fn reopen(
        &self,
        _: ContainerId,
        _: &LocalRecord,
    ) -> Result<Box<dyn SandboxInstance>, Box<WireFailure>> {
        Err(WireFailure::local(RuntimeError::Unsupported(
            "Docker requires a Docker deployment record",
        )))
    }
    fn reopen_stopped(
        &self,
        _: ContainerId,
        _: &LocalRecord,
    ) -> Result<Box<dyn SandboxInstance>, Box<WireFailure>> {
        Err(WireFailure::local(RuntimeError::Unsupported(
            "Docker requires a Docker deployment record",
        )))
    }
}
impl SandboxInstance for Sandbox {
    fn identity(&self) -> ContainerId {
        self.identity()
    }
    fn endpoint(&self) -> Result<SocketAddr, Box<WireFailure>> {
        self.endpoint()
    }
    fn run(
        &self,
        request: &ExecRequest,
        stdout: &mut (dyn Write + Send),
        stderr: &mut (dyn Write + Send),
    ) -> Result<ExecExit, Box<WireFailure>> {
        super::provider_exec::run(self, request, stdout, stderr)
    }
    fn track(&self, _: u32, _: u64) -> Result<(), Box<WireFailure>> {
        // Container teardown owns all descendants; no host process is adopted.
        Ok(())
    }
    fn log_tail(&self, lines: u16, cap: usize) -> Result<Vec<u8>, Box<WireFailure>> {
        self.log_tail(lines, cap)
    }
    fn inspect(&self) -> Result<SandboxInspection, Box<WireFailure>> {
        self.inspect()
    }
    fn stop(&mut self, seconds: u16) -> Result<(), Box<WireFailure>> {
        self.stop(seconds)
    }
    fn delete(&mut self) -> Result<(), Box<WireFailure>> {
        self.delete()
    }
    fn scratch_directory(&self) -> String {
        "/tmp".into()
    }
    fn local_record(&self) -> Option<LocalRecord> {
        None
    }
}
