//! Linux namespace runtime slot; implemented by the namespaces backend.
use super::sandbox::Sandbox;
use crate::backend::native::{Local, LocalPaths};
use crate::provider::{
    IsolationModel, LaunchFailure, LaunchPhase, LaunchRequest, MountDriver, ProviderCapabilities,
    SandboxRecord, SearchPathPolicy,
};
use crate::{ContainerId, RuntimeError, WireFailure};
use std::path::Path;

const UNSUPPORTED: &str = "the Linux namespaces Sandbox backend";

/// A Linux namespace runtime over one local root directory.
#[derive(Clone, Debug)]
pub struct NamespaceProvider {
    local: Local,
}

impl NamespaceProvider {
    /// Refuses with `Unsupported` before any effect in the interface-only build.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        let _ = (
            Local::new(root),
            crate::backend::json::Json::<&[u8]>::text_into,
        );
        Err(RuntimeError::Unsupported(UNSUPPORTED))
    }

    /// The named Sandbox's paths, for its daemon setup.
    pub fn paths(&self, name: &str) -> Result<LocalPaths, RuntimeError> {
        self.local.clone().naming(name)?.paths()
    }

    /// What this backend supports.
    pub fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            name: "namespaces-linux",
            isolation: IsolationModel::MicroVm,
            mount_driver: MountDriver::Fuse,
            requires_image: true,
            search_path_policy: SearchPathPolicy::FixedContainer,
            supports_resource_limits: false,
            direct_store_access: true,
        }
    }

    /// Starts one Sandbox once; a failed start keeps the acknowledged owner.
    pub fn launch(&self, _request: LaunchRequest) -> Result<Sandbox, Box<LaunchFailure>> {
        Err(Box::new(LaunchFailure {
            phase: LaunchPhase::Validate,
            instance: None,
            cause: RuntimeError::Unsupported(UNSUPPORTED),
        }))
    }

    /// Reopen starts nothing: it checks the recorded init process, its cgroup
    /// and its listener.
    pub fn reopen(
        &self,
        _id: ContainerId,
        _record: &SandboxRecord,
    ) -> Result<Sandbox, Box<WireFailure>> {
        Err(WireFailure::local(RuntimeError::Unsupported(UNSUPPORTED)))
    }

    /// A known stopped record admits only the existing runtime's single Delete.
    pub fn reopen_stopped(
        &self,
        _id: ContainerId,
        _record: &SandboxRecord,
    ) -> Result<Sandbox, Box<WireFailure>> {
        Err(WireFailure::local(RuntimeError::Unsupported(UNSUPPORTED)))
    }
}
