//! The existing local runtime behind the provider port; no alternate process engine.
use super::{Local, LocalPhase, LocalProcess, Sandbox};
use crate::provider::{
    ExecExit, IsolationModel, LaunchFailure, LaunchPhase, LaunchRequest, LocalRecord, MountDriver,
    ProviderCapabilities, ProviderLaunchOptions, SandboxInspection, SandboxInstance,
    SandboxProvider, SearchPathPolicy,
};
use crate::{ContainerId, ExecRequest, RuntimeError, WireFailure};
use std::{io::Write, net::SocketAddr, path::Path};

#[derive(Clone, Debug)]
pub struct NativeMacosProvider {
    local: Local,
}
/// The same runtime owner and Darwin process facts as the original native route.
pub type NativeMacosInstance = Sandbox;
impl NativeMacosProvider {
    /// Refuses other platforms before accessing the root or launching anything.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        if !cfg!(target_os = "macos") {
            return Err(RuntimeError::Unsupported("native macOS provider"));
        }
        Ok(Self {
            local: Local::native(root)?,
        })
    }
    pub fn paths(&self, name: &str) -> Result<super::LocalPaths, RuntimeError> {
        self.local.clone().naming(name)?.paths()
    }
}
impl SandboxProvider for NativeMacosProvider {
    fn capabilities(&self) -> ProviderCapabilities {
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
    fn launch(
        &self,
        request: LaunchRequest,
    ) -> Result<Box<dyn SandboxInstance>, Box<LaunchFailure>> {
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
        } = request.options;
        command_identity.check().map_err(fail)?;
        if command_identity.uid != nix::unistd::Uid::effective().as_raw()
            || command_identity.gid != nix::unistd::Gid::effective().as_raw()
            || environment.iter().any(|(key, value)| {
                key.is_empty()
                    || key.contains(['=', '\0'])
                    || value.contains('\0')
                    || key == "PATH"
                    || (key == "LAYERFS_KERNEL" && value != "nfs")
            })
        {
            return Err(fail(RuntimeError::Protocol(
                "native command identity/environment",
            )));
        }
        environment.retain(|(key, _)| key != "LAYERFS_KERNEL");
        environment.push(("LAYERFS_KERNEL".into(), "nfs".into()));
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
            .map(|sandbox| Box::new(sandbox) as Box<dyn SandboxInstance>)
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
                        .map(|s| Box::new(s) as Box<dyn SandboxInstance>),
                    cause: failure.cause,
                })
            })
    }
    fn reopen(
        &self,
        id: ContainerId,
        record: &LocalRecord,
    ) -> Result<Box<dyn SandboxInstance>, Box<WireFailure>> {
        let paths = self.paths(&record.name).map_err(WireFailure::local)?;
        Sandbox::reopen(
            paths.directory,
            id,
            process(record),
            record.port,
            record.labels.clone(),
        )
        .map(|s| Box::new(s) as Box<dyn SandboxInstance>)
    }
    fn reopen_stopped(
        &self,
        id: ContainerId,
        record: &LocalRecord,
    ) -> Result<Box<dyn SandboxInstance>, Box<WireFailure>> {
        let paths = self.paths(&record.name).map_err(WireFailure::local)?;
        Ok(Box::new(Sandbox::stopped(
            paths.directory,
            id,
            process(record),
            record.port,
            record.labels.clone(),
        )))
    }
}
fn process(record: &LocalRecord) -> LocalProcess {
    LocalProcess {
        pid: record.pid,
        start: record.start_time,
        boot: record.boot,
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
        let exit = self.run(request, stdout, stderr)?;
        Ok(ExecExit {
            code: exit.code,
            pid: Some(exit.pid),
            stdout_bytes: exit.stdout,
            stderr_bytes: exit.stderr,
        })
    }
    fn track(&self, pid: u32, start: u64) -> Result<(), Box<WireFailure>> {
        self.track(pid, start)
    }
    fn log_tail(&self, lines: u16, cap: usize) -> Result<Vec<u8>, Box<WireFailure>> {
        self.log_tail(lines, cap)
    }
    fn inspect(&self) -> Result<SandboxInspection, Box<WireFailure>> {
        self.reap();
        Ok(SandboxInspection {
            identity: self.identity(),
            running: self.process().is_some_and(LocalProcess::alive),
            exit_code: None,
            process_id: self.process().map(|p| p.pid),
            scratch_bytes: None,
        })
    }
    fn stop(&mut self, seconds: u16) -> Result<(), Box<WireFailure>> {
        self.stop(seconds)
    }
    fn delete(&mut self) -> Result<(), Box<WireFailure>> {
        self.delete()
    }
    fn scratch_directory(&self) -> String {
        self.tmp().display().to_string()
    }
    fn local_record(&self) -> Option<LocalRecord> {
        let p = self.process()?;
        Some(LocalRecord {
            name: self.directory().file_name()?.to_str()?.to_owned(),
            pid: p.pid,
            start_time: p.start,
            boot: p.boot,
            port: self.port(),
            labels: self.labels().to_vec(),
        })
    }
}
