//! One `docker` CLI call on the machine's Engine socket, as today's
//! `platform/docker/index.js` `socketRun` makes it.
use crate::platform::error::MachineError;
use crate::setup::probe::first_line;
use crate::setup::spec::Spec;
use crate::setup::tool;
use std::process::Command;

/// `docker -H unix://<socket> args...`.
pub(super) fn docker(spec: &Spec, args: &[&str]) -> Result<Command, MachineError> {
    let mut command = tool::command(&spec.tools, &tool::find(&spec.tools, "docker")?, ["-H"]);
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

// `docker` network: the host port `docker port` reports.
/// The host port of `docker port`'s answer (`127.0.0.1:55012`, maybe a second IPv6 line).
pub(crate) fn published_port(answer: &str) -> Option<u16> {
    answer
        .lines()
        .find_map(|line| line.rsplit_once(':')?.1.trim().parse().ok())
}
