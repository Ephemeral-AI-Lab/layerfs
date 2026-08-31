use crate::ContainerId;
use std::fmt;
use std::fs;
use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::time::Duration;

const DAEMON_PORT: &str = "41273/tcp";
const CAPABILITY_PATH: &str = "/run/layerfs/capability";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContainerLimits {
    pub memory_bytes: u64,
    pub cpus: u16,
    pub pids: u32,
}

impl Default for ContainerLimits {
    fn default() -> Self {
        Self {
            memory_bytes: 512 * 1024 * 1024,
            cpus: 2,
            pids: 512,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerCreate {
    pub name: String,
    pub image: String,
    pub limits: ContainerLimits,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatedContainer {
    pub id: ContainerId,
    pub name: String,
    pub image: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerStatus {
    pub id: ContainerId,
    pub name: String,
    pub image: String,
    pub running: bool,
    pub privileged: bool,
    pub fuse_device: bool,
    pub sys_admin: bool,
    pub host_binds: u64,
    pub memory_bytes: u64,
    pub nano_cpus: u64,
    pub pids: u32,
}

#[derive(Clone)]
pub struct ContainerBinding {
    pub(crate) id: ContainerId,
    pub(crate) owner: Arc<layerfs_daemon::Owner>,
    pub(crate) fuse_host: String,
}

impl ContainerBinding {
    pub fn container_id(&self) -> &ContainerId {
        &self.id
    }
}

pub struct RunningContainer {
    pub id: ContainerId,
    pub name: String,
    pub endpoint: SocketAddr,
    binding: ContainerBinding,
}

impl RunningContainer {
    pub fn binding(&self) -> ContainerBinding {
        self.binding.clone()
    }
}

pub struct ContainerManager {
    runtime_root: PathBuf,
}

impl ContainerManager {
    pub fn open(runtime_root: impl AsRef<Path>) -> ContainerResult<Self> {
        let runtime_root = runtime_root.as_ref().to_owned();
        fs::create_dir_all(&runtime_root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&runtime_root, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self { runtime_root })
    }

    pub fn create(&self, request: ContainerCreate) -> ContainerResult<CreatedContainer> {
        validate_create(&request)?;
        let memory = request.limits.memory_bytes.to_string();
        let cpus = request.limits.cpus.to_string();
        let pids = request.limits.pids.to_string();
        let output = docker([
            "create",
            "--name",
            &request.name,
            "--device",
            "/dev/fuse",
            "--cap-add",
            "SYS_ADMIN",
            "--pids-limit",
            &pids,
            "--memory",
            &memory,
            "--cpus",
            &cpus,
            "--publish",
            "127.0.0.1::41273",
            "--label",
            "dev.layerfs.managed=true",
            &request.image,
        ])?;
        let id = exact_id(text(&output.stdout).trim())?;
        Ok(CreatedContainer {
            id,
            name: request.name,
            image: request.image,
        })
    }

    pub fn start(&self, container: &str) -> ContainerResult<RunningContainer> {
        let status = self.status(container)?;
        if !status.running {
            docker(["start", container])?;
        }
        self.connect(container)
    }

    pub fn connect(&self, container: &str) -> ContainerResult<RunningContainer> {
        let status = self.status(container)?;
        if !status.running {
            return Err(ContainerError::NotRunning);
        }
        let endpoint = self.endpoint(container)?;
        let capability = self.capability(container, &status.id)?;
        let owner = retry_owner(endpoint, capability)?;
        Ok(RunningContainer {
            id: status.id.clone(),
            name: status.name,
            endpoint,
            binding: ContainerBinding {
                id: status.id,
                owner,
                fuse_host: "host.docker.internal".to_owned(),
            },
        })
    }

    pub fn status(&self, container: &str) -> ContainerResult<ContainerStatus> {
        validate_selector(container)?;
        let id = exact_id(&inspect(container, "{{.Id}}")?)?;
        let name = inspect(container, "{{.Name}}")?
            .trim_start_matches('/')
            .to_owned();
        let image = inspect(container, "{{.Config.Image}}")?;
        let running = parse_bool(&inspect(container, "{{.State.Running}}")?)?;
        let privileged = parse_bool(&inspect(container, "{{.HostConfig.Privileged}}")?)?;
        let fuse_device = inspect(
            container,
            "{{range .HostConfig.Devices}}{{if eq .PathOnHost \"/dev/fuse\"}}true{{end}}{{end}}",
        )? == "true";
        let sys_admin = inspect(container, "{{json .HostConfig.CapAdd}}")?.contains("SYS_ADMIN");
        let host_binds = parse_u64(&inspect(container, "{{len .HostConfig.Binds}}")?)?;
        let memory_bytes = parse_u64(&inspect(container, "{{.HostConfig.Memory}}")?)?;
        let nano_cpus = parse_u64(&inspect(container, "{{.HostConfig.NanoCpus}}")?)?;
        let pids = parse_u64(&inspect(container, "{{.HostConfig.PidsLimit}}")?)?
            .try_into()
            .map_err(|_| ContainerError::InvalidResponse("PID limit"))?;
        Ok(ContainerStatus {
            id,
            name,
            image,
            running,
            privileged,
            fuse_device,
            sys_admin,
            host_binds,
            memory_bytes,
            nano_cpus,
            pids,
        })
    }

    pub fn stop(&self, container: &str) -> ContainerResult<ContainerStatus> {
        let status = self.status(container)?;
        if status.running {
            docker(["stop", container])?;
        }
        self.status(container)
    }

    pub fn remove(&self, container: &str) -> ContainerResult<()> {
        let status = self.status(container)?;
        if status.running {
            return Err(ContainerError::Running);
        }
        docker(["rm", container])?;
        let capability = self.capability_file(&status.id);
        if capability.exists() {
            fs::remove_file(capability)?;
        }
        Ok(())
    }

    fn endpoint(&self, container: &str) -> ContainerResult<SocketAddr> {
        let output = docker(["port", container, DAEMON_PORT])?;
        let body = text(&output.stdout);
        let value = body
            .lines()
            .find(|line| line.starts_with("127.0.0.1:"))
            .ok_or(ContainerError::InvalidResponse("daemon endpoint"))?;
        value
            .parse()
            .map_err(|_| ContainerError::InvalidResponse("daemon endpoint"))
    }

    fn capability(&self, container: &str, id: &ContainerId) -> ContainerResult<[u8; 32]> {
        let path = self.capability_file(id);
        let mut copied = false;
        for _ in 0..100 {
            if docker_status([
                "cp",
                &format!("{container}:{CAPABILITY_PATH}"),
                path_str(&path)?,
            ])? {
                copied = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if !copied {
            return Err(ContainerError::NotReady);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file() || metadata.len() != 32 {
            return Err(ContainerError::InvalidResponse("daemon capability"));
        }
        let mut capability = [0; 32];
        fs::File::open(path)?.read_exact(&mut capability)?;
        Ok(capability)
    }

    fn capability_file(&self, id: &ContainerId) -> PathBuf {
        self.runtime_root.join(format!("{}.capability", id.0))
    }
}

#[derive(Debug)]
pub enum ContainerError {
    InvalidInput(&'static str),
    InvalidResponse(&'static str),
    Docker(String),
    NotReady,
    NotRunning,
    Running,
    Io(std::io::Error),
}

pub type ContainerResult<T> = std::result::Result<T, ContainerError>;

impl fmt::Display for ContainerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ContainerError {}

impl From<std::io::Error> for ContainerError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

fn retry_owner(
    endpoint: SocketAddr,
    capability: [u8; 32],
) -> ContainerResult<Arc<layerfs_daemon::Owner>> {
    for _ in 0..100 {
        match layerfs_daemon::connect_tcp(endpoint, capability) {
            Ok(owner) => return Ok(owner),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    Err(ContainerError::NotReady)
}

fn validate_create(request: &ContainerCreate) -> ContainerResult<()> {
    validate_selector(&request.name)?;
    if request.image.is_empty()
        || request.image.len() > 512
        || !request.image.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'/' | b':' | b'_' | b'-' | b'@')
        })
    {
        return Err(ContainerError::InvalidInput("container image"));
    }
    if !(64 * 1024 * 1024..=64 * 1024 * 1024 * 1024).contains(&request.limits.memory_bytes)
        || request.limits.cpus == 0
        || request.limits.cpus > 256
        || !(32..=65_535).contains(&request.limits.pids)
    {
        return Err(ContainerError::InvalidInput("container limits"));
    }
    Ok(())
}

fn validate_selector(value: &str) -> ContainerResult<()> {
    if value.is_empty()
        || value.len() > 255
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Err(ContainerError::InvalidInput("container name or ID"))
    } else {
        Ok(())
    }
}

fn inspect(container: &str, format: &str) -> ContainerResult<String> {
    validate_selector(container)?;
    let output = docker(["inspect", "--format", format, container])?;
    Ok(text(&output.stdout).trim().to_owned())
}

fn exact_id(value: &str) -> ContainerResult<ContainerId> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(ContainerId(value.to_ascii_lowercase()))
    } else {
        Err(ContainerError::InvalidResponse("container ID"))
    }
}

fn parse_bool(value: &str) -> ContainerResult<bool> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ContainerError::InvalidResponse("Docker boolean")),
    }
}

fn parse_u64(value: &str) -> ContainerResult<u64> {
    value
        .parse()
        .map_err(|_| ContainerError::InvalidResponse("Docker integer"))
}

fn docker<const N: usize>(args: [&str; N]) -> ContainerResult<Output> {
    let output = Command::new("docker").args(args).output()?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(ContainerError::Docker(limited(&output.stderr)))
    }
}

fn docker_status<const N: usize>(args: [&str; N]) -> ContainerResult<bool> {
    Ok(Command::new("docker").args(args).output()?.status.success())
}

fn limited(bytes: &[u8]) -> String {
    text(&bytes[..bytes.len().min(4096)]).trim().to_owned()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn path_str(path: &Path) -> ContainerResult<&str> {
    path.to_str()
        .ok_or(ContainerError::InvalidInput("container runtime path"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_input_and_exact_identity_are_bounded() {
        let request = ContainerCreate {
            name: "agent-runtime".to_owned(),
            image: "layerfs-runtime:dev".to_owned(),
            limits: ContainerLimits::default(),
        };
        assert!(validate_create(&request).is_ok());
        assert!(exact_id(&"a1".repeat(32)).is_ok());
        assert!(exact_id("agent-runtime").is_err());
        assert!(validate_selector("wrong/container").is_err());
    }
}
