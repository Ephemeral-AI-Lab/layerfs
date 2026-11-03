//! Docker's existing one-attempt runtime as a concrete backend.
use super::{Docker, Sandbox, SandboxRequest};
use crate::{ContainerId, RuntimeError, WireFailure};
use crate::{LaunchFailure, LaunchPhase, LaunchRequest, ProviderLaunchOptions, SandboxRecord};
use std::fs::File;

/// Negotiation and all transfers use the supplied client's original policy and version cache.
#[derive(Clone, Debug)]
pub struct DockerProvider {
    docker: Docker,
}
impl DockerProvider {
    /// Negotiation and all transfers use the supplied client's original policy and version cache.
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }
    pub fn launch(&self, request: LaunchRequest) -> Result<Sandbox, Box<LaunchFailure>> {
        let mut phase = LaunchPhase::Validate;
        let mut instance = None;
        let result = (|| {
            let ProviderLaunchOptions::Docker {
                image,
                store_volume,
                port,
                connect_host,
            } = request.options
            else {
                return Err(RuntimeError::Unsupported(
                    "Docker requires Docker launch options",
                ));
            };
            if let Some(host) = &connect_host {
                if host.is_empty() || host.contains(['\0', ' ', '\t', '\r', '\n']) {
                    return Err(RuntimeError::Protocol("Docker connect_host format"));
                }
            }
            let deployment = SandboxRequest {
                image,
                store_volume,
                port,
                labels: request.labels,
            };
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
            Ok(()) => Ok(instance.expect("launched")),
            Err(cause) => Err(Box::new(LaunchFailure {
                phase,
                instance: instance.map(|sandbox| Box::new(crate::Sandbox::Docker(sandbox))),
                cause,
            })),
        }
    }
    pub fn reopen(
        &self,
        id: ContainerId,
        record: &SandboxRecord,
    ) -> Result<Sandbox, Box<WireFailure>> {
        let Some(record) = record.docker() else {
            return Err(WireFailure::local(RuntimeError::Unsupported(
                "Docker requires a Docker deployment record",
            )));
        };
        Sandbox::reopen(self.docker.clone(), id, record.clone().into())
    }
    pub fn reopen_stopped(
        &self,
        id: ContainerId,
        record: &SandboxRecord,
    ) -> Result<Sandbox, Box<WireFailure>> {
        let Some(record) = record.docker() else {
            return Err(WireFailure::local(RuntimeError::Unsupported(
                "Docker requires a Docker deployment record",
            )));
        };
        let request: SandboxRequest = record.clone().into();
        request.check().map_err(WireFailure::local)?;
        Ok(Sandbox::stopped(self.docker.clone(), id, request))
    }
}
