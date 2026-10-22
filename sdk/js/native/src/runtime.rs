//! A Sandbox's runtime: a Docker container (the default), or with
//! `deploy.sandbox` `local` (LAYERFS_SANDBOX=local) a daemon process on this
//! kernel, for a VM or CI box that is itself the isolation boundary. Both run
//! the same daemon and answer the same calls; only how it is started, how
//! commands are launched and how it is observed differ. With `native`
//! (LAYERFS_SANDBOX=native) the local daemon serves its Workspaces over
//! loopback NFS to this host's own client and commands run as this user,
//! with no root (macOS). The local runtimes run on Linux and macOS;
//! elsewhere selecting one is refused as Unsupported.
use crate::job::{failed, refused};
use crate::output::Spool;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_sandbox::backend::docker;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_sandbox::backend::local;
use layerfs_sandbox::{CommandIdentity, ContainerId, WireFailure};
use std::net::SocketAddr;

/// A local Sandbox record's `runtime`, and the image it records (a Docker
/// record has no `runtime`).
pub(crate) const LOCAL: &str = "local";
/// A native Sandbox record's `runtime` (LAYERFS_SANDBOX=native): a local
/// daemon serving its Workspaces over loopback NFS, commands as this user.
pub(crate) const NATIVE: &str = "native";

/// One Sandbox's runtime owner.
pub(crate) enum Runtime {
    Docker(docker::Sandbox),
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Local(local::Sandbox),
}

/// Whether the deployment selects a local daemon: `deploy.sandbox` unset or
/// `docker` is Docker, `local` and `native` are a local daemon; anything
/// else is refused before any effect.
pub(crate) fn local(deploy: &SdkValue) -> Result<bool, SdkRefusal> {
    match deploy.get_text("sandbox") {
        None | Some("docker") => Ok(false),
        Some(LOCAL) | Some(NATIVE) => Ok(true),
        Some(other) => Err(refused(
            "InvalidInput",
            format!("deploy.sandbox (LAYERFS_SANDBOX) is docker or local (or native), not {other}"),
        )),
    }
}

/// Whether the deployment selects the native route: a local daemon whose
/// Workspaces this host's own NFS client mounts (`LAYERFS_KERNEL=nfs`), and
/// whose commands run as this user, unconfined. No root, no Docker, no FUSE.
pub(crate) fn native(deploy: &SdkValue) -> bool {
    deploy.get_text("sandbox") == Some(NATIVE)
}

/// What a new Workspace of a deployment is placed by: its runtime and, for
/// Docker, its image (`deploy.image`).
#[derive(Debug)]
pub(crate) struct Placement {
    /// The record `runtime` it takes: none for Docker.
    runtime: Option<&'static str>,
    image: Option<String>,
}

/// The call's placement; none without a deployment (no filter, as before).
pub(crate) fn placement(args: &SdkValue) -> Result<Option<Placement>, SdkRefusal> {
    let Some(deploy) = args.get("deploy") else {
        return Ok(None);
    };
    let runtime = local(deploy)?.then(|| if native(deploy) { NATIVE } else { LOCAL });
    Ok(Some(Placement {
        runtime,
        image: runtime
            .is_none()
            .then(|| deploy.get_text("image").map(str::to_owned))
            .flatten(),
    }))
}

impl Placement {
    /// Whether a Sandbox record may take this Workspace: the same runtime
    /// (the record's `runtime`, which a Docker record does not have) and, for
    /// Docker, the same image. The image string alone does not tell them
    /// apart: `local` is also a valid Docker image name.
    pub(crate) fn admits(&self, record: &SdkValue) -> bool {
        let runtime = record.get_text("runtime");
        runtime == self.runtime
            && (runtime.is_some()
                || self.image.is_none()
                || record.get_text("image") == self.image.as_deref())
    }
}

impl Runtime {
    /// The recorded identity: the container's, or a local Sandbox's own.
    pub(crate) fn identity(&self) -> ContainerId {
        match self {
            Self::Docker(sandbox) => sandbox.identity(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Local(sandbox) => sandbox.identity(),
        }
    }
    /// The image it runs; `local` for a local Sandbox.
    pub(crate) fn image(&self) -> &str {
        match self {
            Self::Docker(sandbox) => &sandbox.request().image,
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Local(_) => LOCAL,
        }
    }
    /// Its daemon's endpoint while it runs, observed once; a container or
    /// daemon found gone or stopped is [`WireFailure::container_gone`] or
    /// [`WireFailure::container_stopped`].
    pub(crate) fn endpoint(&self) -> Result<SocketAddr, Box<WireFailure>> {
        match self {
            Self::Docker(sandbox) => sandbox.endpoint(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Local(sandbox) => sandbox.endpoint(),
        }
    }
    /// Where its commands keep scratch files: `/tmp` in a container, the
    /// Sandbox's own `tmp` directory for a local one (deleted with it), so
    /// a local Sandbox leaves nothing in this machine's `/tmp`.
    pub(crate) fn scratch(&self) -> String {
        match self {
            Self::Docker(_) => "/tmp".to_owned(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Local(sandbox) => sandbox.tmp().display().to_string(),
        }
    }
    /// Deletes a Sandbox found stopped, once.
    pub(crate) fn delete(&mut self) -> Result<(), Box<WireFailure>> {
        match self {
            Self::Docker(sandbox) => sandbox.delete(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Local(sandbox) => sandbox.delete(),
        }
    }
    /// Names a background process a command started in a session of its own
    /// (`pid`, started at `start`), so it ends with a local Sandbox as it
    /// ends with a container.
    #[cfg_attr(
        not(any(target_os = "linux", target_os = "macos")),
        allow(unused_variables)
    )]
    pub(crate) fn track(&self, pid: u32, start: &str) -> Result<(), SdkRefusal> {
        match self {
            Self::Docker(_) => Ok(()),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Local(sandbox) => {
                let start = start.parse::<u64>().map_err(failed)?;
                sandbox.track(pid, start).map_err(failed)
            }
        }
    }
    /// One command to its end: exit code, stdout, stderr. `spool` sends
    /// every non-empty stream to its file instead of keeping it inline.
    pub(crate) fn run(
        &self,
        identity: CommandIdentity,
        arguments: Vec<String>,
        environment: Vec<String>,
        directory: &str,
        spool: bool,
    ) -> Result<(i32, Spool, Spool), SdkRefusal> {
        let (mut stdout, mut stderr) = (Spool::new(spool), Spool::new(spool));
        let code = match self {
            Self::Docker(sandbox) => {
                let created = layerfs_api::internal::exec(
                    sandbox,
                    identity,
                    arguments,
                    environment,
                    directory.to_owned(),
                    false,
                )
                .map_err(failed)?;
                let attached = created.start().map_err(failed)?;
                let (_input, output, handle) = attached.into_parts().map_err(failed)?;
                output.copy_to(&mut stdout, &mut stderr).map_err(failed)?;
                let inspected = handle.inspect().map_err(failed)?;
                inspected
                    .known_root_exit()
                    .ok_or_else(|| refused("Unknown", "the command's exit code is not known"))?
            }
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Local(sandbox) => {
                let request = docker::ExecRequest {
                    identity,
                    arguments,
                    environment,
                    directory: directory.to_owned(),
                    stdin: false,
                };
                sandbox
                    .run(&request, &mut stdout, &mut stderr)
                    .map_err(failed)?
                    .code
            }
        };
        Ok((code, stdout, stderr))
    }
}

#[cfg(test)]
#[path = "../tests/unit/runtime.rs"]
mod runtime_tests;
