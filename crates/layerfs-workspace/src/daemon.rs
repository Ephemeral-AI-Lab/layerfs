#[cfg(unix)]
use crate::{ExecutionId, WorkspaceId};
use crate::{WorkspaceError, WorkspaceResult};
#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExecutionRoute {
    DockerEngine,
    Daemon,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MountRoute {
    Docker,
    Daemon,
}

pub(crate) struct DaemonConfiguration {
    pub(crate) route: ExecutionRoute,
    pub(crate) mount_route: MountRoute,
    pub(crate) owner: Option<DaemonOwner>,
}

#[cfg(unix)]
pub(crate) struct DaemonOwner {
    client: std::sync::Arc<layerfs_daemon::Owner>,
    container_id: Option<String>,
    fuse_host: Option<String>,
}

#[cfg(not(unix))]
pub(crate) struct DaemonOwner;

#[cfg(unix)]
pub(crate) struct DaemonExec(layerfs_daemon::Exec);

#[cfg(unix)]
pub(crate) struct DaemonMount(layerfs_daemon::Mount);

#[cfg(unix)]
pub(crate) use layerfs_daemon::Event as DaemonEvent;

pub(crate) fn configure() -> WorkspaceResult<DaemonConfiguration> {
    let route = match std::env::var("LAYERFS_EXEC_TRANSPORT").as_deref() {
        Ok("daemon") => ExecutionRoute::Daemon,
        Ok("docker-engine") | Err(std::env::VarError::NotPresent) => ExecutionRoute::DockerEngine,
        _ => return Err(WorkspaceError::InvalidExecution),
    };
    let mount_route = match std::env::var("LAYERFS_FUSE_TRANSPORT") {
        Ok(value) => select_mount_route(Some(&value), route)?,
        Err(std::env::VarError::NotPresent) => select_mount_route(None, route)?,
        Err(_) => return Err(WorkspaceError::InvalidPlacement),
    };
    let authenticate = route == ExecutionRoute::Daemon
        || mount_route == MountRoute::Daemon
        || std::env::var("LAYERFS_DAEMON_AUTH").as_deref() == Ok("1");
    #[cfg(unix)]
    let container_id = configured_daemon_container(authenticate)?;
    #[cfg(unix)]
    let owner = authenticate
        .then(layerfs_daemon::prepare_owner)
        .transpose()?
        .map(|client| DaemonOwner {
            client,
            container_id,
            fuse_host: None,
        });
    #[cfg(not(unix))]
    {
        if authenticate {
            return Err(WorkspaceError::InvalidExecution);
        }
    }
    #[cfg(not(unix))]
    let owner = None;
    Ok(DaemonConfiguration {
        route,
        mount_route,
        owner,
    })
}

#[cfg(unix)]
pub(crate) fn configure_binding(
    binding: crate::ContainerBinding,
) -> WorkspaceResult<DaemonConfiguration> {
    if !valid_daemon_container_id(&binding.id.0) || binding.fuse_host.is_empty() {
        return Err(WorkspaceError::InvalidPlacement);
    }
    Ok(DaemonConfiguration {
        route: ExecutionRoute::Daemon,
        mount_route: MountRoute::Daemon,
        owner: Some(DaemonOwner {
            client: binding.owner,
            container_id: Some(binding.id.0),
            fuse_host: Some(binding.fuse_host),
        }),
    })
}

#[cfg(not(unix))]
pub(crate) fn configure_binding(
    _binding: crate::ContainerBinding,
) -> WorkspaceResult<DaemonConfiguration> {
    Err(WorkspaceError::InvalidPlacement)
}

#[cfg(unix)]
fn configured_daemon_container(authenticate: bool) -> WorkspaceResult<Option<String>> {
    if !authenticate || std::env::var_os("LAYERFS_DAEMON_TCP_ENDPOINT").is_none() {
        return Ok(None);
    }
    let container_id = std::env::var("LAYERFS_DAEMON_CONTAINER_ID")
        .map_err(|_| WorkspaceError::InvalidPlacement)?;
    if !valid_daemon_container_id(&container_id) {
        return Err(WorkspaceError::InvalidPlacement);
    }
    Ok(Some(container_id))
}

#[cfg(unix)]
fn valid_daemon_container_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn select_mount_route(
    value: Option<&str>,
    execution: ExecutionRoute,
) -> WorkspaceResult<MountRoute> {
    match value {
        Some("daemon") => Ok(MountRoute::Daemon),
        Some("docker") => Ok(MountRoute::Docker),
        None if execution == ExecutionRoute::Daemon => Ok(MountRoute::Daemon),
        None => Ok(MountRoute::Docker),
        Some(_) => Err(WorkspaceError::InvalidPlacement),
    }
}

impl crate::Workspaces {
    pub(crate) fn daemon_mount_owner(&self) -> WorkspaceResult<Option<&DaemonOwner>> {
        match self.mount_route {
            MountRoute::Docker => Ok(None),
            MountRoute::Daemon => self
                .daemon
                .as_ref()
                .map(Some)
                .ok_or(WorkspaceError::InvalidPlacement),
        }
    }
}

#[cfg(unix)]
impl DaemonOwner {
    pub(crate) fn accepts(&self, container: &crate::ContainerId) -> bool {
        self.container_id
            .as_ref()
            .is_none_or(|expected| expected == &container.0)
    }

    pub(crate) fn fuse_host(&self) -> Option<&str> {
        self.fuse_host.as_deref()
    }

    pub(crate) fn start(
        &self,
        workspace_id: WorkspaceId,
        execution_id: ExecutionId,
        cwd: &Path,
        argv: &[OsString],
    ) -> std::io::Result<DaemonExec> {
        use std::os::unix::ffi::OsStrExt;
        self.client
            .start(
                workspace_id.bytes(),
                execution_id.bytes(),
                cwd.as_os_str().as_bytes().to_vec(),
                argv.iter()
                    .map(|value| value.as_os_str().as_bytes().to_vec())
                    .collect(),
            )
            .map(DaemonExec)
    }

    pub(crate) fn mount(
        &self,
        workspace_id: WorkspaceId,
        root: &Path,
        endpoint: &str,
        capability: [u8; 32],
    ) -> std::io::Result<DaemonMount> {
        use std::os::unix::ffi::OsStrExt;
        self.client
            .mount(
                workspace_id.bytes(),
                root.as_os_str().as_bytes().to_vec(),
                endpoint.as_bytes().to_vec(),
                capability,
            )
            .map(DaemonMount)
    }

    pub(crate) fn start_resource_sample(
        &self,
        workspace_id: WorkspaceId,
    ) -> std::io::Result<layerfs_daemon::ResourceSampleClock> {
        self.client.start_resource_sample(workspace_id.bytes())
    }

    pub(crate) fn finish_resource_sample(
        &self,
        workspace_id: WorkspaceId,
        t0_unix_ns: u64,
        t3_unix_ns: u64,
        uncertainty_ns: u64,
    ) -> std::io::Result<layerfs_daemon::protocol::CgroupResourceSample> {
        self.client.finish_resource_sample(
            workspace_id.bytes(),
            t0_unix_ns,
            t3_unix_ns,
            uncertainty_ns,
        )
    }
}

#[cfg(unix)]
impl DaemonMount {
    pub(crate) fn mountinfo(&self) -> &[u8] {
        self.0.mountinfo()
    }

    pub(crate) fn close(&mut self) -> std::io::Result<()> {
        self.0.close().map(|_| ())
    }

    pub(crate) fn disconnect(&self) -> std::io::Result<()> {
        self.0.disconnect()
    }
}

#[cfg(unix)]
impl DaemonExec {
    pub(crate) fn reader(&self) -> std::io::Result<layerfs_daemon::Stream> {
        self.0.try_clone()
    }

    pub(crate) fn stop(&mut self) -> std::io::Result<()> {
        self.0.stop()
    }

    pub(crate) fn disconnect(&self) -> std::io::Result<()> {
        self.0.disconnect()
    }

    pub(crate) fn read(stream: &mut layerfs_daemon::Stream) -> std::io::Result<DaemonEvent> {
        layerfs_daemon::Exec::read(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_exec_selects_the_required_live_daemon_mount_by_default() {
        assert_eq!(
            select_mount_route(None, ExecutionRoute::Daemon).unwrap(),
            MountRoute::Daemon
        );
        assert_eq!(
            select_mount_route(None, ExecutionRoute::DockerEngine).unwrap(),
            MountRoute::Docker
        );
        assert_eq!(
            select_mount_route(Some("docker"), ExecutionRoute::Daemon).unwrap(),
            MountRoute::Docker
        );
    }

    #[cfg(unix)]
    #[test]
    fn tcp_daemon_container_id_is_exact_and_bounded() {
        assert!(valid_daemon_container_id("layerfs-daemon_1.0"));
        assert!(!valid_daemon_container_id(""));
        assert!(!valid_daemon_container_id("wrong/container"));
        assert!(!valid_daemon_container_id(&"a".repeat(256)));
    }
}
