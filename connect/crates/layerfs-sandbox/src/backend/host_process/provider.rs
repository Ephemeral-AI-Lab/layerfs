//! The macOS native runtime as a concrete backend; no alternate process engine.
//! Linux has no native provider (#533 O27): its local Sandboxes run in
//! namespaces.
use super::sandbox::Sandbox;
use crate::local::{Local, LocalPhase, LocalProcess};
use crate::{ContainerId, RuntimeError, WireFailure};
use crate::{
    IsolationModel, LaunchFailure, LaunchPhase, LaunchRequest, LocalRecord, MountDriver,
    ProviderCapabilities, ProviderLaunchOptions, SandboxRecord, SearchPathPolicy,
};
use std::path::Path;

/// Native macOS processes with loopback NFS and the caller's identity.
#[derive(Clone, Debug)]
pub struct HostProcessProvider {
    local: Local,
}
impl HostProcessProvider {
    /// Selects `root`; nothing is launched.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        Ok(Self {
            local: Local::native(root)?,
        })
    }
    pub fn paths(&self, name: &str) -> Result<crate::local::LocalPaths, RuntimeError> {
        self.local.clone().naming(name)?.paths()
    }
    pub fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            name: "native-macos",
            isolation: IsolationModel::HostProcess,
            mount_driver: MountDriver::LoopbackNfs,
            requires_image: false,
            search_path_policy: SearchPathPolicy::InheritHost,
            supports_resource_limits: false,
            direct_store_access: true,
        }
    }
    pub fn launch(&self, request: LaunchRequest) -> Result<Sandbox, Box<LaunchFailure>> {
        let fail = |cause| {
            Box::new(LaunchFailure {
                phase: LaunchPhase::Validate,
                instance: None,
                cause,
            })
        };
        let ProviderLaunchOptions::Local {
            command_identity,
            environment,
        } = request.options
        else {
            return Err(fail(RuntimeError::Unsupported(
                "native provider requires local launch options",
            )));
        };
        command_identity.check().map_err(fail)?;
        if command_identity.uid != nix::unistd::Uid::effective().as_raw()
            || command_identity.gid != nix::unistd::Gid::effective().as_raw()
            || environment.iter().any(|(key, value)| {
                key.is_empty() || key.contains(['=', '\0']) || value.contains('\0') || key == "PATH"
            })
        {
            return Err(fail(RuntimeError::Protocol(
                "native command identity/environment",
            )));
        }
        let local = self
            .local
            .clone()
            .naming(&request.name)
            .map_err(fail)?
            .environment(environment);
        local
            .create(
                &request.daemon_executable,
                &request.daemon_config,
                request.startup_timeout,
                request.labels,
            )
            .map_err(|failure| {
                Box::new(LaunchFailure {
                    phase: match failure.phase {
                        LocalPhase::Validate => LaunchPhase::Validate,
                        LocalPhase::Create => LaunchPhase::Create,
                        LocalPhase::Start => LaunchPhase::Start,
                        LocalPhase::Listener => LaunchPhase::Listener,
                    },
                    instance: failure
                        .sandbox
                        .map(|sandbox| Box::new(crate::Sandbox::HostProcess(sandbox))),
                    cause: failure.cause,
                })
            })
    }
    pub fn reopen(
        &self,
        id: ContainerId,
        record: &SandboxRecord,
    ) -> Result<Sandbox, Box<WireFailure>> {
        let Some(record) = record.local() else {
            return Err(WireFailure::local(RuntimeError::Unsupported(
                "native provider requires a local record",
            )));
        };
        let paths = self.paths(&record.name).map_err(WireFailure::local)?;
        Sandbox::reopen(
            paths.directory,
            id,
            process(record),
            record.port,
            record.labels.clone(),
        )
    }
    pub fn reopen_stopped(
        &self,
        id: ContainerId,
        record: &SandboxRecord,
    ) -> Result<Sandbox, Box<WireFailure>> {
        let Some(record) = record.local() else {
            return Err(WireFailure::local(RuntimeError::Unsupported(
                "native provider requires a local record",
            )));
        };
        let paths = self.paths(&record.name).map_err(WireFailure::local)?;
        Ok(Sandbox::stopped(
            paths.directory,
            id,
            process(record),
            record.port,
            record.labels.clone(),
        ))
    }
}
fn process(record: &LocalRecord) -> LocalProcess {
    LocalProcess {
        pid: record.pid,
        start: record.start_time,
        boot: record.boot,
    }
}
