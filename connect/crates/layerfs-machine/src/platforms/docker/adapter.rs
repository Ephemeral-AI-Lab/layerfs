//! `docker`'s adapter: the built-in machine on Docker's kernel.
use super::{guest, start, status, stop};
use crate::adapter::{refused, Adapter};
use crate::error::MachineError;
use crate::platform::{ExecutorKind, HostOs, Platform};
use crate::probe::first_line;
use crate::setup::guest::Guest;
use crate::setup::{tool, Status};
use crate::spec::{Bundle, Running, Spec, Tools};

pub(crate) struct Docker;
pub(crate) static ADAPTER: Docker = Docker;

impl Adapter for Docker {
    fn probe(&self, tools: &Tools, host: HostOs, missing: &mut Vec<String>) -> Option<String> {
        if host == HostOs::Windows {
            missing.push("host:wsl2".into());
            return None;
        }
        let Some(docker) = tool::lookup(tools, "docker") else {
            missing.push("tool:docker".into());
            return None;
        };
        let mut command = tool::command(&docker, ["-H"]);
        command
            .arg(format!("unix://{}", tools.docker_socket.display()))
            .args(["version", "--format", "{{.Server.Version}}"]);
        let version = first_line(command, false);
        if version.is_none() {
            missing.push(format!("host:{}", tools.docker_socket.display()));
        }
        version
    }
    fn guest(&self, spec: &Spec) -> Result<Guest, MachineError> {
        if Platform::Docker.executor(HostOs::current()) == ExecutorKind::Worker {
            guest(spec)
        } else {
            Err(refused(spec, "runs in-process; its files are the host's"))
        }
    }
    fn create(&self, spec: &Spec, _bundle: &Bundle) -> Result<Running, MachineError> {
        Err(refused(spec, "is built in and never created"))
    }
    fn remove(&self, spec: &Spec) -> Result<(), MachineError> {
        Err(refused(spec, "is built in and never removed"))
    }
    fn start(&self, spec: &Spec, bundle: &Bundle) -> Result<Option<Running>, MachineError> {
        start(spec, bundle).map(Some)
    }
    fn stop(&self, spec: &Spec) -> Result<(), MachineError> {
        stop(spec)
    }
    fn status(&self, spec: &Spec) -> Result<Status, MachineError> {
        status(spec)
    }
    fn host_identity(&self, spec: &Spec) -> Result<String, MachineError> {
        Err(refused(spec, "has no ssh host"))
    }
}
