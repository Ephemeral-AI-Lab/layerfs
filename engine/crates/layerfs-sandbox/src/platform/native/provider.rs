//! The existing native runtime as a concrete backend; no alternate process engine.
use super::{Local, LocalPhase, LocalProcess, Sandbox};
use crate::{ContainerId, RuntimeError, WireFailure};
use crate::{
    IsolationModel, LaunchFailure, LaunchPhase, LaunchRequest, LocalRecord, MountDriver,
    ProviderCapabilities, ProviderLaunchOptions, SandboxRecord, SearchPathPolicy,
};
use std::path::Path;

/// The daemon's kernel route setting (`fuse` or `nfs`). Only the provider
/// names it, from the platform it serves; a caller that names it is refused
/// (#527 step 6), and the daemon checks the value it receives.
pub(crate) const KERNEL_ROUTE: &str = "LAYERFS_KERNEL";

#[derive(Clone, Debug)]
pub struct NativeProvider<const LINUX: bool> {
    local: Local,
}
/// Native macOS processes with loopback NFS and the caller's identity.
pub type NativeMacosProvider = NativeProvider<false>;
/// Native Linux processes with FUSE and a separate nonroot command identity.
pub type NativeLinuxProvider = NativeProvider<true>;
impl<const LINUX: bool> NativeProvider<LINUX> {
    /// Refuses other platforms before accessing the root or launching anything.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        if (LINUX && !cfg!(target_os = "linux")) || (!LINUX && !cfg!(target_os = "macos")) {
            return Err(RuntimeError::Unsupported("native provider platform"));
        }
        if LINUX && !nix::unistd::Uid::effective().is_root() {
            return Err(RuntimeError::Unsupported(
                "native Linux provider requires root",
            ));
        }
        Ok(Self {
            local: if LINUX {
                Local::new(root)?
            } else {
                Local::native(root)?
            },
        })
    }
    pub fn paths(&self, name: &str) -> Result<super::LocalPaths, RuntimeError> {
        self.local.clone().naming(name)?.paths()
    }
    pub fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            name: if LINUX {
                "native-linux"
            } else {
                "native-macos"
            },
            isolation: IsolationModel::HostProcess,
            mount_driver: if LINUX {
                MountDriver::Fuse
            } else {
                MountDriver::LoopbackNfs
            },
            requires_image: false,
            search_path_policy: if LINUX {
                SearchPathPolicy::FixedContainer
            } else {
                SearchPathPolicy::InheritHost
            },
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
            mut environment,
        } = request.options
        else {
            return Err(fail(RuntimeError::Unsupported(
                "native provider requires local launch options",
            )));
        };
        command_identity.check().map_err(fail)?;
        if (!LINUX
            && (command_identity.uid != nix::unistd::Uid::effective().as_raw()
                || command_identity.gid != nix::unistd::Gid::effective().as_raw()))
            || environment.iter().any(|(key, value)| {
                key.is_empty()
                    || key.contains(['=', '\0'])
                    || value.contains('\0')
                    || key == "PATH"
                    || key == KERNEL_ROUTE
            })
        {
            return Err(fail(RuntimeError::Protocol(
                "native command identity/environment",
            )));
        }
        // The provider alone chooses the kernel route (#527 step 6).
        environment.push((
            KERNEL_ROUTE.into(),
            if LINUX { "fuse" } else { "nfs" }.into(),
        ));
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
                        .map(|sandbox| Box::new(crate::Sandbox::Native(sandbox))),
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
