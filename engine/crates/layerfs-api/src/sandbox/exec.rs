//! Optional ordinary Sandbox launch at the acknowledged mounted directory.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

fn refusal(code: &'static str, message: impl Into<String>) -> SdkRefusal {
    SdkRefusal {
        code: code.to_owned(),
        message: message.into(),
        detail: SdkValue::Null,
    }
}

/// Joins an optional project-root `cwd` to a mounted Workspace directory.
/// Validates that `cwd`, when given, is a canonical absolute path (no empty,
/// `.` or `..` components, and no trailing slash except `/`), and verifies
/// that the joined directory remains contained within `workspace`.
pub fn exec_directory(workspace: &str, cwd: Option<&str>) -> Result<String, SdkRefusal> {
    let Some(cwd) = cwd else {
        return Ok(workspace.to_owned());
    };
    let canonical = cwd == "/"
        || (cwd.starts_with('/')
            && !cwd.ends_with('/')
            && cwd
                .split('/')
                .skip(1)
                .all(|part| !part.is_empty() && part != "." && part != ".."));
    if !canonical {
        return Err(refusal(
            "InvalidInput",
            "workspace.exec cwd must be a canonical absolute path without empty, dot or parent components",
        ));
    }
    let directory = if cwd == "/" {
        workspace.to_owned()
    } else {
        format!("{}/{}", workspace.trim_end_matches('/'), &cwd[1..])
    };
    if !Path::new(&directory).starts_with(Path::new(workspace)) {
        return Err(refusal(
            "Failed",
            "workspace.exec cwd escaped the Workspace",
        ));
    }
    Ok(directory)
}

#[cfg(unix)]
use crate::{ManagedSandbox, MountedWorkspace, WorkspaceApi};
#[cfg(unix)]
use layerfs_sandbox::{
    backend::docker::{ExecCreateFailure, ExecCreated, ExecRequest, Sandbox},
    CommandIdentity,
};

#[cfg(unix)]
impl WorkspaceApi<'_> {
    /// Creates one ordinary command at this mount using the deployment's admitted identity.
    /// The returned Sandbox runtime owner owns Start, streams, exit status and cancellation.
    /// This sends no daemon request, registers no Exec and performs no Commit or unmount.
    pub fn exec(
        sandbox: &ManagedSandbox,
        workspace: &MountedWorkspace,
        arguments: Vec<String>,
        environment: Vec<String>,
        stdin: bool,
    ) -> Result<ExecCreated, Box<ExecCreateFailure>> {
        launch(
            &sandbox.runtime,
            sandbox.command_identity,
            arguments,
            environment,
            workspace.ready.directory.clone(),
            stdin,
        )
    }
}

#[cfg(unix)]
pub(crate) fn launch(
    sandbox: &Sandbox,
    identity: CommandIdentity,
    arguments: Vec<String>,
    environment: Vec<String>,
    directory: String,
    stdin: bool,
) -> Result<ExecCreated, Box<ExecCreateFailure>> {
    sandbox.create_exec(ExecRequest {
        identity,
        arguments,
        environment,
        directory,
        stdin,
    })
}
