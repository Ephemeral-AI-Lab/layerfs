use crate::{ContainerId, WorkspaceError, WorkspaceResult, WorkspaceSessionId};
use layerfs_fuse::{ProxyHost, SharedPort};
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

pub(crate) struct DockerProjection {
    proxy: ProxyHost,
    child: Child,
    container: ContainerId,
    root: PathBuf,
    helper: String,
    control: String,
    cleaned: bool,
}

impl DockerProjection {
    pub(crate) fn attach(
        id: WorkspaceSessionId,
        container: ContainerId,
        root: PathBuf,
        port: SharedPort,
        runtime: &Path,
    ) -> WorkspaceResult<Self> {
        let proxy = ProxyHost::start(port)?;
        let helper = format!("/var/tmp/layerfs-owned/layerfs-fuse-{id}");
        let control = format!("/tmp/layerfs-control-{id}.sock");
        let local_helper = helper_path()?;
        require_success(
            Command::new("docker")
                .arg("exec")
                .arg(&container.0)
                .arg("test")
                .arg("-c")
                .arg("/dev/fuse"),
        )?;
        require_success(
            Command::new("docker")
                .arg("exec")
                .arg(&container.0)
                .arg("mkdir")
                .arg("-p")
                .arg(&root),
        )?;
        require_success(
            Command::new("docker")
                .arg("exec")
                .arg(&container.0)
                .arg("mkdir")
                .arg("-p")
                .arg("/var/tmp/layerfs-owned"),
        )?;
        require_success(
            Command::new("docker")
                .arg("cp")
                .arg(&local_helper)
                .arg(format!("{}:{helper}", container.0)),
        )?;
        require_success(
            Command::new("docker")
                .arg("exec")
                .arg(&container.0)
                .arg("chmod")
                .arg("0555")
                .arg(&helper),
        )?;
        let gateway = endpoint_host(&container)?;
        let mut child = Command::new("docker")
            .arg("exec")
            .arg(&container.0)
            .arg(&helper)
            .arg(format!("{gateway}:{}", proxy.port()))
            .arg(hex(proxy.capability()))
            .arg(&root)
            .arg(&control)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or(WorkspaceError::InvalidPlacement)?;
        let (ready_send, ready_receive) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut lines = std::io::BufReader::new(stdout).lines();
            let _ = ready_send.send(lines.next().transpose());
        });
        match ready_receive.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(Ok(Some(line))) if line == "READY" => {}
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(stderr) = child.stderr.take() {
                    retain_stderr(stderr, runtime.join("fuse.stderr"));
                }
                return Err(WorkspaceError::InvalidPlacement);
            }
        }
        if let Some(stderr) = child.stderr.take() {
            drain_stderr(stderr, runtime.join("fuse.stderr"));
        }
        Ok(Self {
            proxy,
            child,
            container,
            root,
            helper,
            control,
            cleaned: false,
        })
    }

    pub(crate) fn end(mut self) -> WorkspaceResult<()> {
        self.cleanup();
        Ok(())
    }

    pub(crate) fn healthy(&self) -> bool {
        self.proxy.healthy()
    }

    pub(crate) fn failure(&self) -> Option<(&'static str, layerfs_fuse::PortError)> {
        self.proxy.failure()
    }

    pub(crate) fn pause(&self) -> WorkspaceResult<()> {
        self.control("pause")
    }

    pub(crate) fn resume(&self) -> WorkspaceResult<()> {
        self.control("resume")
    }

    fn control(&self, command: &str) -> WorkspaceResult<()> {
        let status = Command::new("docker")
            .arg("exec")
            .arg(&self.container.0)
            .arg(&self.helper)
            .arg("--control")
            .arg(&self.control)
            .arg(command)
            .status()?;
        if status.success() {
            Ok(())
        } else if command == "pause" {
            Err(WorkspaceError::WorkspaceBusy)
        } else {
            Err(WorkspaceError::InvalidPlacement)
        }
    }

    pub(crate) fn make_read_only(&self) -> WorkspaceResult<()> {
        require_success(
            Command::new("docker")
                .arg("exec")
                .arg(&self.container.0)
                .arg("mount")
                .arg("-o")
                .arg("remount,ro")
                .arg(&self.root),
        )
    }

    fn cleanup(&mut self) {
        if self.cleaned {
            return;
        }
        self.cleaned = true;
        let _ = Command::new("docker")
            .arg("exec")
            .arg(&self.container.0)
            .arg("umount")
            .arg(&self.root)
            .status();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = Command::new("docker")
            .arg("exec")
            .arg(&self.container.0)
            .arg("rm")
            .arg("-f")
            .arg(&self.helper)
            .status();
        let _ = Command::new("docker")
            .arg("exec")
            .arg(&self.container.0)
            .arg("rm")
            .arg("-f")
            .arg(&self.control)
            .status();
    }
}

impl Drop for DockerProjection {
    fn drop(&mut self) {
        self.cleanup();
    }
}

fn helper_path() -> WorkspaceResult<PathBuf> {
    if let Some(path) = std::env::var_os("LAYERFS_FUSE_HELPER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    let path = std::env::current_exe()?
        .parent()
        .ok_or(WorkspaceError::InvalidPlacement)?
        .join("layerfs-fuse");
    if path.is_file() {
        Ok(path)
    } else {
        Err(WorkspaceError::InvalidPlacement)
    }
}

fn require_success(command: &mut Command) -> WorkspaceResult<()> {
    if command.status()?.success() {
        Ok(())
    } else {
        Err(WorkspaceError::InvalidPlacement)
    }
}

fn gateway(container: &ContainerId) -> Option<String> {
    let output = Command::new("docker")
        .arg("inspect")
        .arg("-f")
        .arg("{{range .NetworkSettings.Networks}}{{.Gateway}}{{end}}")
        .arg(&container.0)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn endpoint_host(container: &ContainerId) -> WorkspaceResult<String> {
    if let Some(host) = std::env::var_os("LAYERFS_FUSE_HOST") {
        let host = host.to_string_lossy().into_owned();
        if host.is_empty()
            || host.len() > 253
            || !host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        {
            return Err(WorkspaceError::InvalidPlacement);
        }
        return Ok(host);
    }
    Ok(if cfg!(target_os = "macos") {
        "host.docker.internal".to_owned()
    } else {
        gateway(container).unwrap_or_else(|| "host.docker.internal".to_owned())
    })
}

fn hex(bytes: [u8; 32]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

fn drain_stderr<R: Read + Send + 'static>(input: R, path: PathBuf) {
    std::thread::spawn(move || {
        retain_stderr(input, path);
    });
}

fn retain_stderr<R: Read>(input: R, path: PathBuf) {
    let mut bytes = Vec::new();
    let _ = input.take(1024 * 1024).read_to_end(&mut bytes);
    let _ = std::fs::write(path, bytes);
}
