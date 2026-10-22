//! A Sandbox's runtime: a Docker container (the default), or with
//! `deploy.sandbox` `local` (LAYERFS_SANDBOX=local) a daemon process on this
//! kernel, for a VM or CI box that is itself the isolation boundary. Both run
//! the same daemon and answer the same calls; only how it is started, how
//! commands are launched and how it is observed differ.
use crate::job::{failed, refused};
use crate::output::Spool;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_sandbox::backend::{docker, local};
use layerfs_sandbox::{CommandIdentity, ContainerId, WireFailure};
use std::net::SocketAddr;

/// The image a local Sandbox records: placement keeps local and Docker
/// Sandboxes of one project apart by it.
pub(crate) const LOCAL: &str = "local";

/// One Sandbox's runtime owner.
pub(crate) enum Runtime {
    Docker(docker::Sandbox),
    Local(local::Sandbox),
}

/// Whether the deployment selects the local runtime: `deploy.sandbox` unset
/// or `docker` is Docker, `local` is local; anything else is refused before
/// any effect.
pub(crate) fn local(deploy: &SdkValue) -> Result<bool, SdkRefusal> {
    match deploy.get_text("sandbox") {
        None | Some("docker") => Ok(false),
        Some("local") => Ok(true),
        Some(other) => Err(refused(
            "InvalidInput",
            format!("deploy.sandbox (LAYERFS_SANDBOX) is docker or local, not {other}"),
        )),
    }
}

/// The image a new Workspace of this deployment is placed by: `local` for
/// the local runtime, else `deploy.image`.
pub(crate) fn image(args: &SdkValue) -> Result<Option<String>, SdkRefusal> {
    let Some(deploy) = args.get("deploy") else {
        return Ok(None);
    };
    Ok(if local(deploy)? {
        Some(LOCAL.to_owned())
    } else {
        deploy.get_text("image").map(str::to_owned)
    })
}

impl Runtime {
    /// The recorded identity: the container's, or a local Sandbox's own.
    pub(crate) fn identity(&self) -> ContainerId {
        match self {
            Self::Docker(sandbox) => sandbox.identity(),
            Self::Local(sandbox) => sandbox.identity(),
        }
    }
    /// The image it runs; `local` for a local Sandbox.
    pub(crate) fn image(&self) -> &str {
        match self {
            Self::Docker(sandbox) => &sandbox.request().image,
            Self::Local(_) => LOCAL,
        }
    }
    /// Its daemon's endpoint while it runs, observed once; a container or
    /// daemon found gone or stopped is [`WireFailure::container_gone`] or
    /// [`WireFailure::container_stopped`].
    pub(crate) fn endpoint(&self) -> Result<SocketAddr, Box<WireFailure>> {
        match self {
            Self::Docker(sandbox) => sandbox.endpoint(),
            Self::Local(sandbox) => sandbox.endpoint(),
        }
    }
    /// Where its commands keep scratch files: `/tmp` in a container, the
    /// Sandbox's own `tmp` directory for a local one (deleted with it), so
    /// a local Sandbox leaves nothing in this machine's `/tmp`.
    pub(crate) fn scratch(&self) -> String {
        match self {
            Self::Docker(_) => "/tmp".to_owned(),
            Self::Local(sandbox) => sandbox.tmp().display().to_string(),
        }
    }
    /// Deletes a Sandbox found stopped, once.
    pub(crate) fn delete(&mut self) -> Result<(), Box<WireFailure>> {
        match self {
            Self::Docker(sandbox) => sandbox.delete(),
            Self::Local(sandbox) => sandbox.delete(),
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
