//! A Sandbox's runtime: Docker (default), or `LAYERFS_SANDBOX=native` on
//! Linux (root FUSE daemon, nonroot commands) and macOS (loopback NFS,
//! caller identity). Both use the same authenticated daemon control pipeline
//! and the same concrete `layerfs_sandbox::Sandbox` handle.
use crate::job::{failed, refused};
use crate::output::Spool;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_sandbox::provider::SandboxRecord;
#[cfg(target_os = "linux")]
use layerfs_sandbox::Image;
use layerfs_sandbox::{CommandIdentity, Sandbox};
#[cfg(target_os = "linux")]
use std::path::PathBuf;

/// The image a native Sandbox records and reports, and the `runtime` of a
/// record from the former `local` runtime, which is never used (a Docker
/// record has no `runtime`).
pub(crate) const LOCAL: &str = "local";
/// A native Sandbox record's `runtime` (LAYERFS_SANDBOX=native): a local
/// daemon serving FUSE on Linux or loopback NFS on macOS.
pub(crate) const NATIVE: &str = "native";
/// A namespace Sandbox record's `runtime` (LAYERFS_SANDBOX_BACKEND=namespaces,
/// Linux): a local daemon in its own mount, PID and IPC namespaces and cgroup.
pub(crate) const NAMESPACES: &str = "namespaces";

/// Which local backend a native deployment runs inside its Sandbox directory:
/// the plain native one, or the Linux namespace one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Backend {
    Native,
    Namespaces,
}

/// The local backend the deployment selects: `deploy.sandboxBackend`
/// (LAYERFS_SANDBOX_BACKEND), `native` when unset. The namespace backend is
/// Linux only; anywhere else selecting it is refused before any effect.
pub(crate) fn backend(deploy: &SdkValue) -> Result<Backend, SdkRefusal> {
    match deploy.get_text("sandboxBackend") {
        None | Some(NATIVE) => Ok(Backend::Native),
        Some(NAMESPACES) if cfg!(target_os = "linux") => Ok(Backend::Namespaces),
        Some(NAMESPACES) => Err(refused(
            "Unsupported",
            "deploy.sandboxBackend (LAYERFS_SANDBOX_BACKEND) namespaces is Linux only; use native",
        )),
        Some(other) => Err(refused(
            "InvalidInput",
            format!(
                "deploy.sandboxBackend (LAYERFS_SANDBOX_BACKEND) is native or namespaces, not {other}"
            ),
        )),
    }
}

/// Whether the deployment selects a local daemon: `deploy.sandbox` unset or
/// `docker` is Docker, `native` is a local daemon; anything else (the
/// former `local` included) is refused before any effect.
pub(crate) fn local(deploy: &SdkValue) -> Result<bool, SdkRefusal> {
    match deploy.get_text("sandbox") {
        None | Some("docker") => Ok(false),
        Some(NATIVE) => Ok(true),
        Some(other) => Err(refused(
            "InvalidInput",
            format!("deploy.sandbox (LAYERFS_SANDBOX) is docker or native (Linux or macOS), not {other}"),
        )),
    }
}

/// Whether the deployment selects the native route: FUSE on Linux,
/// loopback NFS on macOS, with no Docker.
pub(crate) fn native(deploy: &SdkValue) -> bool {
    deploy.get_text("sandbox") == Some(NATIVE)
}

/// The image a Sandbox reports: its container's, the digest a namespaces
/// Sandbox's root is an overlay of, or `local` for a native one.
pub(crate) fn image(runtime: &Sandbox) -> String {
    match runtime.record() {
        Some(SandboxRecord::Docker(docker)) => docker.image,
        #[cfg(target_os = "linux")]
        Some(SandboxRecord::Namespaces(record)) => record.image().as_str().to_owned(),
        _ => LOCAL.to_owned(),
    }
}

/// The image a namespaces deployment's Sandbox root is an overlay of:
/// `deploy.image` (LAYERFS_IMAGE), the same `sha256:<64 hex>` digest the
/// Docker backend uses, and `deploy.imageArchive` (LAYERFS_IMAGE_ARCHIVE),
/// the docker-archive in the bundle that holds it. Both are read before any
/// effect, so a bundle without its image refuses the start.
#[cfg(target_os = "linux")]
pub(crate) fn namespaces_image(deploy: &SdkValue) -> Result<(Image, PathBuf), SdkRefusal> {
    let digest = deploy.get_text("image").ok_or_else(|| {
        refused(
            "InvalidInput",
            "deploy.image is not configured (LAYERFS_IMAGE); a namespaces Sandbox's root is the              image that digest names",
        )
    })?;
    let image = Image::parse(digest)
        .map_err(|error| refused("InvalidInput", format!("deploy.image: {error}")))?;
    let archive = deploy.get_text("imageArchive").ok_or_else(|| {
        refused(
            "InvalidInput",
            "deploy.imageArchive is not configured (LAYERFS_IMAGE_ARCHIVE); a namespaces Sandbox              needs the docker-archive the bundle carries",
        )
    })?;
    let archive = PathBuf::from(archive);
    if !archive.is_absolute() {
        return Err(refused(
            "InvalidInput",
            "deploy.imageArchive (LAYERFS_IMAGE_ARCHIVE) must be an absolute path",
        ));
    }
    Ok((image, archive))
}

/// Where its commands keep scratch files: `/tmp` in a container, the
/// Sandbox's own `tmp` directory for a native one (deleted with it), so a
/// native Sandbox leaves nothing in this machine's `/tmp`.
pub(crate) fn scratch(runtime: &Sandbox) -> String {
    runtime.scratch_directory()
}

/// Names a background process a command started in a session of its own
/// (`pid`, started at `start`), so it ends with a native Sandbox as it ends
/// with a container. A container's teardown already owns its descendants.
pub(crate) fn track(runtime: &Sandbox, pid: u32, start: &str) -> Result<(), SdkRefusal> {
    if matches!(runtime, Sandbox::Docker(_)) {
        return Ok(());
    }
    let start = start.parse::<u64>().map_err(failed)?;
    runtime.track(pid, start).map_err(failed)
}

/// One command to its end: exit code, stdout, stderr. `spool` sends every
/// non-empty stream to its file instead of keeping it inline.
pub(crate) fn run(
    runtime: &Sandbox,
    identity: CommandIdentity,
    arguments: Vec<String>,
    environment: Vec<String>,
    directory: &str,
    spool: bool,
) -> Result<(i32, Spool, Spool), SdkRefusal> {
    let (mut stdout, mut stderr) = (Spool::new(spool), Spool::new(spool));
    let request = layerfs_sandbox::ExecRequest {
        identity,
        arguments,
        environment,
        directory: directory.to_owned(),
        stdin: false,
    };
    let exit = runtime
        .run(&request, &mut stdout, &mut stderr)
        .map_err(failed)?;
    Ok((exit.code, stdout, stderr))
}
