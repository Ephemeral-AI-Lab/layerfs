//! `docker`'s adapter: the built-in machine on Docker's kernel, whose worker
//! container runs `namespaces` Sandboxes (D-8).
use super::{guest, start, status, stop};
use crate::adapter::{refused, Adapter};
use crate::error::{Code, MachineError};
use crate::platform::{ExecutorKind, HostOs, Platform};
use crate::probe::first_line;
use crate::reach::{dial, Loopback};
use crate::setup::forward::refuse;
use crate::setup::guest::Guest;
use crate::setup::{tool, Status};
use crate::spec::{Bundle, Forward, Running, Spec, Tools};

/// Why a Sandbox daemon's port is not forwarded: the worker container's
/// port is published when it starts (`docker run -p`), which is the only
/// way Docker publishes one, so a later daemon's port cannot be (D-8).
const FORWARD: &str = "Docker publishes a container's ports only when it starts, so a Sandbox daemon's port in the worker container cannot be published later; its Workspace calls go through the worker";

/// The name the worker container, and the Sandboxes in it, reach the
/// host's loopback by: Docker Desktop's, and on a Linux Engine the
/// `--add-host` the worker is started with.
pub(crate) const DESKTOP_HOST: &str = "host.docker.internal";

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
    fn reach(&self, spec: &Spec, url: &str) -> Result<String, MachineError> {
        dial(spec, url, Loopback::Alias(DESKTOP_HOST))
    }
    fn forward(&self, spec: &Spec, _guest_port: u16) -> Result<Forward, MachineError> {
        Err(refuse(spec, Code::Unsupported, FORWARD))
    }
}
