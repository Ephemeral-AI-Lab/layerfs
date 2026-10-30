//! The Linux namespace runtime as a concrete backend: the same local runtime
//! root and daemon control as the native one, with each Sandbox's daemon in
//! its own mount, PID and IPC namespaces and its own cgroup, and its commands
//! entering them as a nonroot identity. The VM that holds this kernel is the
//! boundary; the namespaces keep Sandboxes on one Store out of each other.
use super::{
    cgroup,
    create::{self, CreateFailure, Options},
    host,
    paths::Paths,
    sandbox::Sandbox,
};
use crate::backend::native::{Local, LocalPaths};
use crate::provider::{
    IsolationModel, LaunchFailure, LaunchPhase, LaunchRequest, MountDriver, ProviderCapabilities,
    ProviderLaunchOptions, SandboxRecord, SearchPathPolicy,
};
use crate::{ContainerId, RuntimeError, WireFailure};
use std::{
    env,
    path::{Path, PathBuf},
};

/// The helper executable, installed beside the daemon it starts.
const HELPER: &str = "layerfs-sandbox-helper";
/// Names the helper explicitly, when it is not beside the daemon.
const HELPER_ENVIRONMENT: &str = "LAYERFS_SANDBOX_HELPER";

/// A Linux namespace runtime over one local root directory.
#[derive(Clone, Debug)]
pub struct NamespaceProvider {
    local: Local,
}

impl NamespaceProvider {
    /// Selects `root`, the parent of every Sandbox directory, and refuses a
    /// host that cannot hold namespace Sandboxes before any effect: not root,
    /// user namespaces not blocked, no cgroup v2, or no `/dev/fuse`.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        let local = Local::new(root)?;
        host::require_root()?;
        host::require_user_namespaces_blocked()?;
        host::require_cgroup_v2()?;
        host::require_fuse()?;
        Ok(Self { local })
    }

    /// The named Sandbox's paths, for its daemon setup.
    pub fn paths(&self, name: &str) -> Result<LocalPaths, RuntimeError> {
        self.local.clone().naming(name)?.paths()
    }

    /// What this backend supports. The isolation boundary is the VM that
    /// holds this one kernel (one per Store); a Sandbox inside it owns its
    /// mount, PID and IPC namespaces and its cgroup, not a VM of its own.
    pub fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            name: "namespaces-linux",
            isolation: IsolationModel::MicroVm,
            mount_driver: MountDriver::Fuse,
            requires_image: false,
            search_path_policy: SearchPathPolicy::FixedContainer,
            supports_resource_limits: false,
            direct_store_access: true,
        }
    }

    /// Starts one Sandbox once; a failed start keeps the acknowledged owner.
    pub fn launch(&self, request: LaunchRequest) -> Result<Sandbox, Box<LaunchFailure>> {
        let fail = |cause| {
            Box::new(LaunchFailure {
                phase: LaunchPhase::Validate,
                instance: None,
                cause,
            })
        };
        let ProviderLaunchOptions::Namespaces {
            command_identity,
            environment,
            store,
        } = request.options.clone()
        else {
            return Err(fail(RuntimeError::Unsupported(
                "namespaces provider requires namespaces launch options",
            )));
        };
        let helper = helper(Some(&request.daemon_executable)).map_err(fail)?;
        self.local.clone().naming(&request.name).map_err(fail)?;
        let options = Options {
            command_identity,
            environment,
            store,
        };
        create::create(&self.local, &helper, &request, &options).map_err(|failure| {
            let CreateFailure {
                phase,
                sandbox,
                cause,
            } = *failure;
            Box::new(LaunchFailure {
                phase: match phase {
                    crate::backend::native::LocalPhase::Validate => LaunchPhase::Validate,
                    crate::backend::native::LocalPhase::Create => LaunchPhase::Create,
                    crate::backend::native::LocalPhase::Start => LaunchPhase::Start,
                    crate::backend::native::LocalPhase::Listener => LaunchPhase::Listener,
                },
                instance: sandbox.map(|sandbox| {
                    Box::new(crate::backend::Sandbox::Namespaces(Box::new(sandbox)))
                }),
                cause,
            })
        })
    }

    /// Reopen starts nothing: it checks the recorded init process, its cgroup
    /// and its listener.
    pub fn reopen(
        &self,
        id: ContainerId,
        record: &SandboxRecord,
    ) -> Result<Sandbox, Box<WireFailure>> {
        let record = record.namespaces().ok_or_else(|| {
            WireFailure::local(RuntimeError::Unsupported(
                "namespaces provider requires a namespaces record",
            ))
        })?;
        let paths = self
            .record_paths(&record.name)
            .map_err(WireFailure::local)?;
        let helper = helper(None).map_err(WireFailure::local)?;
        Sandbox::reopen(
            paths,
            cgroup::path(&id),
            id,
            super::sandbox::process(record),
            record.port,
            record.labels.clone(),
            helper,
        )
    }

    /// A known stopped record admits only the existing runtime's single Delete.
    pub fn reopen_stopped(
        &self,
        id: ContainerId,
        record: &SandboxRecord,
    ) -> Result<Sandbox, Box<WireFailure>> {
        let record = record.namespaces().ok_or_else(|| {
            WireFailure::local(RuntimeError::Unsupported(
                "namespaces provider requires a namespaces record",
            ))
        })?;
        let paths = self
            .record_paths(&record.name)
            .map_err(WireFailure::local)?;
        let helper = helper(None).map_err(WireFailure::local)?;
        Ok(Sandbox::stopped(
            paths,
            cgroup::path(&id),
            id,
            super::sandbox::process(record),
            record.port,
            record.labels.clone(),
            helper,
        ))
    }

    fn record_paths(&self, name: &str) -> Result<Paths, RuntimeError> {
        Paths::of(self.paths(name)?)
    }
}

/// The helper this backend runs: `LAYERFS_SANDBOX_HELPER` when set, else
/// beside `daemon` or beside this process (or its Cargo test runner).
fn helper(daemon: Option<&Path>) -> Result<PathBuf, RuntimeError> {
    if let Some(path) = env::var_os(HELPER_ENVIRONMENT) {
        let path = PathBuf::from(path);
        if let Ok(metadata) = std::fs::metadata(&path) {
            if path.is_absolute() && metadata.is_file() && metadata.len() > 0 {
                return Ok(path);
            }
        }
    }
    let mut candidates = Vec::new();
    if let Some(parent) = daemon.and_then(Path::parent) {
        candidates.push(parent.join(HELPER));
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join(HELPER));
            if let Some(grandparent) = parent.parent() {
                candidates.push(grandparent.join(HELPER));
            }
        }
    }
    for candidate in candidates {
        if let Ok(metadata) = std::fs::metadata(&candidate) {
            if candidate.is_absolute() && metadata.is_file() && metadata.len() > 0 {
                return Ok(candidate);
            }
        }
    }
    Err(RuntimeError::Unsupported(
        "the namespaces Sandbox runtime needs the layerfs-sandbox-helper executable beside the          daemon (or LAYERFS_SANDBOX_HELPER set)",
    ))
}
