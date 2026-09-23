use crate::owner::OwnerConfig;
use layerfs_api_core::SandboxId;
use layerfs_bridge::{
    adapters::native::connection::VerifiedPeer,
    contract::{Code, Failure},
};
use std::{net::SocketAddr, process::Command};

fn run(args: &[&str]) -> Result<String, Failure> {
    let output = Command::new("docker").args(args).output()?;
    if !output.status.success() {
        return Err(Code::Io.into());
    }
    String::from_utf8(output.stdout).map_err(|_| Code::InvalidInput.into())
}

pub(crate) fn valid_image(image: &str) -> bool {
    let digest = image
        .strip_prefix("sha256:")
        .or_else(|| image.rsplit_once("@sha256:").map(|(_, digest)| digest));
    digest.is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

pub(crate) fn launch(
    config: &OwnerConfig,
    image: &str,
    name: &str,
    id: SandboxId,
    container: &str,
    host_port: u16,
) -> Result<(), Failure> {
    let image_actual = run(&["image", "inspect", "--format", "{{.Id}}", image])?;
    if !valid_image(image_actual.trim()) {
        return Err(Code::InvalidInput.into());
    }
    let control_public = VerifiedPeer::from_private(&config.control_private)?;
    let peers = format!("1,{},{},255", hex(control_public.public_key()), u64::MAX);
    let mount_root = format!("type=volume,src={container}-root,dst=/layerfs");
    let publish = format!("127.0.0.1:{host_port}:23456");
    let mut command = Command::new("docker");
    command.args([
        "run",
        "-d",
        "--name",
        container,
        "--label",
        "io.layerfs.owner=agent-sdk",
        "--label",
        &format!("io.layerfs.sandbox-name={name}"),
        "--cpus",
        "2",
        "--memory",
        "512m",
        "--memory-swap",
        "512m",
        "--pids-limit",
        "64",
        "--read-only",
        "--tmpfs",
        "/tmp:rw,nosuid,nodev,size=16m",
        "--device",
        "/dev/fuse",
        "--cap-add",
        "SYS_ADMIN",
        "--security-opt",
        "apparmor=unconfined",
        "--security-opt",
        "no-new-privileges",
        "--add-host",
        "host.docker.internal:host-gateway",
        "--mount",
        &mount_root,
        "--publish",
        &publish,
        "--entrypoint",
        "/layerfs-daemon",
    ]);
    for (key, value) in [
        ("LAYERFS_ENDPOINT", config.service_endpoint.clone()),
        ("LAYERFS_SELECTOR", config.service_selector.to_string()),
        ("LAYERFS_PRIVATE_KEY", hex(&config.service_private)),
        ("LAYERFS_SERVER_KEY", hex(&config.service_public)),
        ("LAYERFS_WORKSPACE_ROOT", "/layerfs".into()),
        ("LAYERFS_WORKSPACE_MAX_COUNT", "2".into()),
        (
            "LAYERFS_WORKSPACE_MEMORY_BUDGET_BYTES",
            (16 * 1024 * 1024).to_string(),
        ),
        (
            "LAYERFS_WORKSPACE_DISK_BUDGET_BYTES",
            (1024 * 1024 * 1024u64).to_string(),
        ),
        ("LAYERFS_CONTROL_LISTEN", "0.0.0.0:23456".into()),
        ("LAYERFS_CONTROL_PEERS", peers),
        ("LAYERFS_SANDBOX_ID", id.to_string()),
        (
            "LAYERFS_TELEMETRY",
            if config.telemetry_run.is_some() {
                "forward"
            } else {
                "off"
            }
            .into(),
        ),
        ("LAYERFS_CONSTRUCTION_WORKERS", "1".into()),
    ] {
        command.env(key, value).args(["--env", key]);
    }
    if let Some(run) = config.telemetry_run {
        let namespace =
            u64::from_be_bytes(id.0[..8].try_into().map_err(|_| Code::InvalidInput)?).max(1);
        for (key, value) in [
            ("LAYERFS_RUN_ID", run.to_string()),
            ("LAYERFS_NAMESPACE", namespace.to_string()),
            ("LAYERFS_TELEMETRY_INTERVAL_MS", "10".into()),
        ] {
            command.env(key, value).args(["--env", key]);
        }
    }
    let output = command
        .args([image, "--idle-sandbox", &config.store.to_string(), "0", "0"])
        .output()?;
    if !output.status.success() {
        return Err(Code::Io.into());
    }
    Ok(())
}

pub(crate) fn port(container: &str) -> Result<SocketAddr, Failure> {
    let result = run(&["port", container, "23456/tcp"])?;
    let endpoint = result
        .trim()
        .parse::<SocketAddr>()
        .map_err(|_| Code::InvalidInput)?;
    if endpoint.ip() != std::net::Ipv4Addr::LOCALHOST || endpoint.port() == 0 {
        return Err(Code::Denied.into());
    }
    Ok(endpoint)
}

pub(crate) fn shell_ready(container: &str) -> Result<(), Failure> {
    let output = Command::new("docker")
        .args(["exec", container, "/bin/sh", "-c", ":"])
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(Code::Unsupported.into())
    }
}

pub(crate) fn running(container: &str) -> Result<bool, Failure> {
    let output = Command::new("docker")
        .args(["inspect", "--format", "{{.State.Running}}", container])
        .output()?;
    Ok(output.status.success() && output.stdout == b"true\n")
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut out, "{byte:02x}").expect("String write");
    }
    out
}
