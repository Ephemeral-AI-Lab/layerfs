use crate::{ContainerId, WorkspaceError, WorkspaceId, WorkspaceResult};
use layerfs_fuse::{ProxyHost, SharedPort};
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[cfg(target_os = "macos")]
const DEFAULT_DAEMON_ENDPOINT_HOST: &str = "host.docker.internal";
#[cfg(all(unix, not(target_os = "macos")))]
const DEFAULT_DAEMON_ENDPOINT_HOST: &str = "127.0.0.1";

const ATTACH_SCRIPT: &str = r#"set -eu
test -c /dev/fuse
created_root=0
if ! test -d "$2"; then mkdir -p -- "$2"; created_root=1; fi
mkdir -p -- /var/tmp/layerfs-owned
chmod 0700 /var/tmp/layerfs-owned
umask 077
start=$(awk '{print $22}' "/proc/$$/stat")
printf '%s %s %s\n' "$$" "$start" "$created_root" > "$5"
if test -n "$6"; then test -x "$6"; ln -- "$6" "$1" 2>/dev/null || cp -- "$6" "$1"; else cat > "$1"; fi
chmod 0555 "$1"
LAYERFS_OWNED_HELPER="$1" LAYERFS_OWNED_ROOT="$2" LAYERFS_OWNED_CAPABILITY="$4" exec "$1" "$3" "$4" "$2""#;

const FALLBACK_CLEANUP_SCRIPT: &str = r#"set -eu
helper=$1
identity=$2
root=$3
capability=$4
mounted() { findmnt -rn -M "$root" >/dev/null; }
if test -f "$identity"; then
  read -r pid start created_root < "$identity"
  case "$pid:$start" in *[!0-9:]*|:*) exit 20;; esac
  case "$created_root" in 0|1) ;; *) exit 20;; esac
  if test -r "/proc/$pid/stat"; then
    test "$(awk '{print $22}' "/proc/$pid/stat")" = "$start"
    test "$(readlink "/proc/$pid/exe")" = "$helper"
    grep -zFx "LAYERFS_OWNED_HELPER=$helper" "/proc/$pid/environ" >/dev/null
    grep -zFx "LAYERFS_OWNED_ROOT=$root" "/proc/$pid/environ" >/dev/null
    grep -zFx "LAYERFS_OWNED_CAPABILITY=$capability" "/proc/$pid/environ" >/dev/null
    if mounted; then umount -- "$root"; fi
    kill "$pid" 2>/dev/null || true
  fi
fi
if mounted; then umount -- "$root"; fi
rm -f -- "$helper" "$identity"
if test "${created_root:-0}" = 1; then rmdir -- "$root" 2>/dev/null || true; fi
! mounted"#;

pub(crate) struct DockerProjection {
    proxy: ProxyHost,
    launcher: ProjectionLauncher,
    container: ContainerId,
    root: PathBuf,
    runtime: PathBuf,
    cleaned: bool,
}

enum ProjectionLauncher {
    Docker {
        child: Child,
        helper: String,
        identity: String,
        capability: String,
    },
    #[cfg(unix)]
    Daemon(crate::daemon::DaemonMount),
}

struct AttachGuard {
    child: Option<Child>,
    container: String,
    root: PathBuf,
    helper: String,
    identity: String,
    capability: String,
    runtime: PathBuf,
}

impl DockerProjection {
    pub(crate) fn attach(
        id: WorkspaceId,
        container: ContainerId,
        root: PathBuf,
        port: SharedPort,
        runtime: &Path,
        daemon: Option<&crate::daemon::DaemonOwner>,
    ) -> WorkspaceResult<Self> {
        #[cfg(unix)]
        if let Some(daemon) = daemon {
            if !daemon.accepts(&container) {
                return Err(WorkspaceError::InvalidPlacement);
            }
            return Self::attach_daemon(id, container, root, port, runtime, daemon);
        }
        #[cfg(not(unix))]
        if daemon.is_some() {
            return Err(WorkspaceError::InvalidPlacement);
        }
        let total_started = std::time::Instant::now();
        let started = std::time::Instant::now();
        let proxy = ProxyHost::start(port)?;
        let proxy_ns = elapsed_ns(started);
        let helper = format!("/var/tmp/layerfs-owned/layerfs-fuse-{id}");
        let identity = format!("{helper}.identity");
        let local_helper = helper_path()?;
        let endpoint_inspect = u64::from(
            !cfg!(target_os = "macos") && std::env::var_os("LAYERFS_FUSE_HOST").is_none(),
        );
        let gateway = endpoint_host(&container)?;
        let endpoint = format!("{gateway}:{}", proxy.port());
        let capability = hex(proxy.capability());
        let container_helper = std::env::var("LAYERFS_CONTAINER_FUSE_HELPER")
            .ok()
            .filter(|path| Path::new(path).is_absolute());
        let started = std::time::Instant::now();
        let child = Command::new("docker")
            .arg("exec")
            .arg("-i")
            .arg(&container.0)
            .args(["/bin/sh", "-c"])
            .arg(ATTACH_SCRIPT)
            .arg("layerfs-attach")
            .arg(&helper)
            .arg(&root)
            .arg(&endpoint)
            .arg(&capability)
            .arg(&identity)
            .arg(container_helper.as_deref().unwrap_or(""))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let docker_setup_ns = elapsed_ns(started);
        let mut guard = AttachGuard {
            child: Some(child),
            container: container.0.clone(),
            root: root.clone(),
            helper: helper.clone(),
            identity: identity.clone(),
            capability: capability.clone(),
            runtime: runtime.to_owned(),
        };
        let started = std::time::Instant::now();
        let mut stdin = guard
            .child_mut()
            .stdin
            .take()
            .ok_or(WorkspaceError::InvalidPlacement)?;
        let copied = if container_helper.is_some() {
            0
        } else {
            std::io::copy(&mut std::fs::File::open(&local_helper)?, &mut stdin)?
        };
        drop(stdin);
        if container_helper.is_none() && copied != std::fs::metadata(&local_helper)?.len() {
            return Err(WorkspaceError::InvalidPlacement);
        }
        let helper_copy_ns = elapsed_ns(started);
        let started = std::time::Instant::now();
        let stdout = guard
            .child_mut()
            .stdout
            .take()
            .ok_or(WorkspaceError::InvalidPlacement)?;
        let (ready_send, ready_receive) = std::sync::mpsc::sync_channel(1);
        let mountinfo_path = runtime.join("mountinfo.txt");
        std::thread::spawn(move || {
            let mut lines = std::io::BufReader::new(stdout).lines();
            let ready = lines.next().transpose();
            let mountinfo = lines.next().transpose().and_then(|line| {
                line.filter(|line| line.starts_with("MOUNTINFO\t"))
                    .ok_or_else(|| std::io::Error::other("missing FUSE mountinfo"))
            });
            let captured = mountinfo.and_then(|line| {
                std::fs::write(mountinfo_path, format!("{}\n", &line[10..])).map(|_| ())
            });
            let _ = ready_send.send(if captured.is_ok() {
                ready
            } else {
                captured.map(|_| None)
            });
        });
        match ready_receive.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(Ok(Some(line))) if line == "READY" => {}
            _ => return Err(WorkspaceError::InvalidPlacement),
        }
        let mount_ready_ns = elapsed_ns(started);
        let total_ns = elapsed_ns(total_started);
        let attributed = proxy_ns
            .saturating_add(docker_setup_ns)
            .saturating_add(helper_copy_ns)
            .saturating_add(mount_ready_ns);
        layerfs_layerstack_store::record_workspace_lifecycle(
            layerfs_layerstack_store::WorkspaceLifecycleReceipt {
                kind: layerfs_layerstack_store::WorkspaceLifecycleKind::Attach,
                total_ns,
                proxy_ns,
                docker_setup_ns,
                helper_copy_ns,
                mount_ready_ns,
                unmount_ns: 0,
                wait_ns: 0,
                cleanup_ns: 0,
                unattributed_ns: total_ns.saturating_sub(attributed),
                docker_calls: 1 + endpoint_inspect,
                ..Default::default()
            },
        )?;
        let mut child = guard.disarm();
        if let Some(stderr) = child.stderr.take() {
            drain_stderr(stderr, runtime.join("fuse.stderr"));
        }
        Ok(Self {
            proxy,
            launcher: ProjectionLauncher::Docker {
                child,
                helper,
                identity,
                capability,
            },
            container,
            root,
            runtime: runtime.to_owned(),
            cleaned: false,
        })
    }

    #[cfg(unix)]
    fn attach_daemon(
        id: WorkspaceId,
        container: ContainerId,
        root: PathBuf,
        port: SharedPort,
        runtime: &Path,
        daemon: &crate::daemon::DaemonOwner,
    ) -> WorkspaceResult<Self> {
        let total_started = std::time::Instant::now();
        let started = std::time::Instant::now();
        let proxy = ProxyHost::start(port)?;
        let proxy_ns = elapsed_ns(started);
        let endpoint = format!(
            "{}:{}",
            daemon
                .fuse_host()
                .map(str::to_owned)
                .map_or_else(daemon_endpoint_host, Ok)?,
            proxy.port()
        );
        let started = std::time::Instant::now();
        let mount = daemon.mount(id, &root, &endpoint, proxy.capability())?;
        let mount_ready_ns = elapsed_ns(started);
        std::fs::write(
            runtime.join("mountinfo.txt"),
            [mount.mountinfo(), b"\n"].concat(),
        )?;
        let total_ns = elapsed_ns(total_started);
        let attributed = proxy_ns.saturating_add(mount_ready_ns);
        layerfs_layerstack_store::record_workspace_lifecycle(
            layerfs_layerstack_store::WorkspaceLifecycleReceipt {
                kind: layerfs_layerstack_store::WorkspaceLifecycleKind::Attach,
                total_ns,
                proxy_ns,
                docker_setup_ns: 0,
                helper_copy_ns: 0,
                mount_ready_ns,
                unmount_ns: 0,
                wait_ns: 0,
                cleanup_ns: 0,
                unattributed_ns: total_ns.saturating_sub(attributed),
                docker_calls: 0,
                ..Default::default()
            },
        )?;
        Ok(Self {
            proxy,
            launcher: ProjectionLauncher::Daemon(mount),
            container,
            root,
            runtime: runtime.to_owned(),
            cleaned: false,
        })
    }

    pub(crate) fn end(&mut self) -> WorkspaceResult<()> {
        if self.cleaned {
            return Ok(());
        }
        #[cfg(unix)]
        if matches!(self.launcher, ProjectionLauncher::Daemon(_)) {
            return self.end_daemon();
        }
        let total_started = std::time::Instant::now();
        let started = std::time::Instant::now();
        if self.proxy.control("shutdown").is_err() {
            let unmount_ns = elapsed_ns(started);
            let (cleanup_ns, wait_ns, verified) = self.fallback("end-fallback");
            self.cleaned = verified;
            let _ = record_end(total_started, unmount_ns, wait_ns, cleanup_ns, 1);
            return Err(WorkspaceError::InvalidPlacement);
        }
        let unmount_ns = elapsed_ns(started);
        let started = std::time::Instant::now();
        let exited = match &mut self.launcher {
            ProjectionLauncher::Docker { child, .. } => {
                child.wait().is_ok_and(|status| status.success())
            }
            #[cfg(unix)]
            ProjectionLauncher::Daemon(_) => unreachable!(),
        };
        let wait_ns = elapsed_ns(started);
        if !exited {
            let (cleanup_ns, fallback_wait_ns, verified) = self.fallback("end-fallback");
            self.cleaned = verified;
            let _ = record_end(
                total_started,
                unmount_ns,
                wait_ns.saturating_add(fallback_wait_ns),
                cleanup_ns,
                1,
            );
            return Err(WorkspaceError::InvalidPlacement);
        }
        self.cleaned = true;
        record_end(total_started, unmount_ns, wait_ns, 0, 0)
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

    pub(crate) fn invalidate_file(&self, node: layerfs_fuse::NodeId) -> WorkspaceResult<()> {
        self.proxy
            .invalidate_file(node)
            .map_err(|_| WorkspaceError::InvalidPlacement)
    }

    pub(crate) fn take_write_metrics(&self) -> WorkspaceResult<layerfs_fuse::FuseWriteMetrics> {
        self.proxy
            .take_write_metrics()
            .map_err(|_| WorkspaceError::InvalidPlacement)
    }

    pub(crate) fn take_read_metrics(&self) -> WorkspaceResult<layerfs_fuse::FuseReadMetrics> {
        self.proxy
            .take_read_metrics()
            .map_err(|_| WorkspaceError::InvalidPlacement)
    }

    fn control(&self, command: &str) -> WorkspaceResult<()> {
        if self.proxy.control(command).is_ok() {
            Ok(())
        } else if command == "pause" {
            Err(WorkspaceError::WorkspaceBusy)
        } else {
            Err(WorkspaceError::InvalidPlacement)
        }
    }

    fn fallback(&mut self, evidence: &str) -> (u64, u64, bool) {
        #[cfg(unix)]
        if let ProjectionLauncher::Daemon(mount) = &mut self.launcher {
            let started = std::time::Instant::now();
            let disconnected = mount.disconnect().is_ok();
            return (elapsed_ns(started), 0, disconnected);
        }
        let (child, helper, identity, capability) = match &mut self.launcher {
            ProjectionLauncher::Docker {
                child,
                helper,
                identity,
                capability,
            } => (child, helper, identity, capability),
            #[cfg(unix)]
            ProjectionLauncher::Daemon(_) => unreachable!(),
        };
        let started = std::time::Instant::now();
        let verified = checked_fallback_cleanup(
            &self.container.0,
            &self.root,
            helper,
            identity,
            capability,
            &self.runtime,
            evidence,
        );
        let cleanup_ns = elapsed_ns(started);
        let started = std::time::Instant::now();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let wait_ns = elapsed_ns(started);
        (cleanup_ns, wait_ns, verified)
    }

    #[cfg(unix)]
    fn end_daemon(&mut self) -> WorkspaceResult<()> {
        let total_started = std::time::Instant::now();
        let started = std::time::Instant::now();
        if self.proxy.control("shutdown").is_err() {
            let unmount_ns = elapsed_ns(started);
            let (cleanup_ns, wait_ns, cleaned) = self.fallback("end-fallback");
            self.cleaned = cleaned;
            let _ = record_end(total_started, unmount_ns, wait_ns, cleanup_ns, 0);
            return Err(WorkspaceError::InfrastructureLost);
        }
        let unmount_ns = elapsed_ns(started);
        let started = std::time::Instant::now();
        let closed = match &mut self.launcher {
            ProjectionLauncher::Daemon(mount) => mount.close().is_ok(),
            ProjectionLauncher::Docker { .. } => unreachable!(),
        };
        let wait_ns = elapsed_ns(started);
        if !closed {
            let (cleanup_ns, fallback_wait_ns, cleaned) = self.fallback("end-fallback");
            self.cleaned = cleaned;
            let _ = record_end(
                total_started,
                unmount_ns,
                wait_ns.saturating_add(fallback_wait_ns),
                cleanup_ns,
                0,
            );
            return Err(WorkspaceError::InfrastructureLost);
        }
        self.cleaned = true;
        record_end(total_started, unmount_ns, wait_ns, 0, 0)
    }
}

impl AttachGuard {
    fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("armed attach child")
    }

    fn disarm(mut self) -> Child {
        self.child.take().expect("armed attach child")
    }

    fn cleanup(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        checked_fallback_cleanup(
            &self.container,
            &self.root,
            &self.helper,
            &self.identity,
            &self.capability,
            &self.runtime,
            "attach-cleanup",
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(stderr) = child.stderr.take() {
            retain_stderr(stderr, self.runtime.join("fuse.stderr"));
        }
        self.child = None;
    }
}

impl Drop for AttachGuard {
    fn drop(&mut self) {
        self.cleanup();
    }
}

impl Drop for DockerProjection {
    fn drop(&mut self) {
        if !self.cleaned {
            let (_, _, verified) = self.fallback("drop-fallback");
            self.cleaned = verified;
        }
    }
}

fn checked_fallback_cleanup(
    container: &str,
    root: &Path,
    helper: &str,
    identity: &str,
    capability: &str,
    runtime: &Path,
    evidence: &str,
) -> bool {
    let output = Command::new("docker")
        .arg("exec")
        .arg(container)
        .args(["/bin/sh", "-c", FALLBACK_CLEANUP_SCRIPT, "layerfs-cleanup"])
        .arg(helper)
        .arg(identity)
        .arg(root)
        .arg(capability)
        .output();
    let Ok(output) = output else {
        return false;
    };
    let _ = std::fs::write(runtime.join(format!("{evidence}.stdout")), &output.stdout);
    let _ = std::fs::write(runtime.join(format!("{evidence}.stderr")), &output.stderr);
    let _ = std::fs::write(
        runtime.join(format!("{evidence}.status")),
        format!("{}\n", output.status.success()),
    );
    output.status.success()
}

fn record_end(
    total_started: std::time::Instant,
    unmount_ns: u64,
    wait_ns: u64,
    cleanup_ns: u64,
    docker_calls: u64,
) -> WorkspaceResult<()> {
    let total_ns = elapsed_ns(total_started);
    let attributed = unmount_ns
        .saturating_add(wait_ns)
        .saturating_add(cleanup_ns);
    layerfs_layerstack_store::record_workspace_lifecycle(
        layerfs_layerstack_store::WorkspaceLifecycleReceipt {
            kind: layerfs_layerstack_store::WorkspaceLifecycleKind::End,
            total_ns,
            proxy_ns: 0,
            docker_setup_ns: 0,
            helper_copy_ns: 0,
            mount_ready_ns: 0,
            unmount_ns,
            wait_ns,
            cleanup_ns,
            unattributed_ns: total_ns.saturating_sub(attributed),
            docker_calls,
            ..Default::default()
        },
    )
    .map_err(Into::into)
}

fn elapsed_ns(started: std::time::Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
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
    if let Some(host) = configured_endpoint_host()? {
        return Ok(host);
    }
    Ok(if cfg!(target_os = "macos") {
        "host.docker.internal".to_owned()
    } else {
        gateway(container).unwrap_or_else(|| "host.docker.internal".to_owned())
    })
}

#[cfg(unix)]
fn daemon_endpoint_host() -> WorkspaceResult<String> {
    Ok(configured_endpoint_host()?.unwrap_or_else(|| DEFAULT_DAEMON_ENDPOINT_HOST.to_owned()))
}

fn configured_endpoint_host() -> WorkspaceResult<Option<String>> {
    let Some(host) = std::env::var_os("LAYERFS_FUSE_HOST") else {
        return Ok(None);
    };
    let host = host.to_string_lossy().into_owned();
    if host.is_empty()
        || host.len() > 253
        || !host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(WorkspaceError::InvalidPlacement);
    }
    Ok(Some(host))
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
