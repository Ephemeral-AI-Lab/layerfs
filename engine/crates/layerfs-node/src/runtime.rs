//! A Sandbox's runtime: Docker (default), or `LAYERFS_SANDBOX=native` on
//! Linux (root FUSE daemon, nonroot commands) and macOS (loopback NFS,
//! caller identity). Both use the same authenticated daemon control pipeline
//! and the same concrete `layerfs_sandbox::Sandbox` handle.
use crate::job::{failed, refused};
use crate::output::Spool;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_sandbox::provider::SandboxRecord;
use layerfs_sandbox::{CommandIdentity, Sandbox};

/// The image a native Sandbox records and reports, and the `runtime` of a
/// record from the former `local` runtime, which is never used (a Docker
/// record has no `runtime`).
pub(crate) const LOCAL: &str = "local";
/// A native Sandbox record's `runtime` (LAYERFS_SANDBOX=native): a local
/// daemon serving FUSE on Linux or loopback NFS on macOS.
pub(crate) const NATIVE: &str = "native";

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

/// The image a Sandbox reports: its container's, or `local` for a native one.
pub(crate) fn image(runtime: &Sandbox) -> String {
    match runtime.record() {
        Some(SandboxRecord::Docker(docker)) => docker.image,
        _ => LOCAL.to_owned(),
    }
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
