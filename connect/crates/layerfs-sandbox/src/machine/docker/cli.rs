//! One `docker` CLI call on the machine's Engine socket, as today's
//! `platform/docker/index.js` `socketRun` makes it.
use crate::machine::error::MachineError;
use crate::machine::probe::first_line;
use crate::machine::spec::Spec;
use crate::machine::tool;
use std::process::Command;

/// `docker -H unix://<socket> args...`.
pub(super) fn docker(spec: &Spec, args: &[&str]) -> Result<Command, MachineError> {
    let mut command = tool::command(&tool::find(&spec.tools, "docker")?, ["-H"]);
    command
        .arg(format!("unix://{}", spec.tools.docker_socket.display()))
        .args(args);
    Ok(command)
}

/// The Engine's version, when it answers.
pub(super) fn engine_version(spec: &Spec) -> Option<String> {
    let command = docker(spec, &["version", "--format", "{{.Server.Version}}"]).ok()?;
    first_line(command, false)
}
