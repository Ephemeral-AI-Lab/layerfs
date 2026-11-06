//! `docker`'s adapter: the built-in machine on Docker's kernel.
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

/// Why `forward` is not where a docker Sandbox's daemon address comes from:
/// the Sandbox's container publishes it (`PortBindings`, host IP
/// `127.0.0.1`, an Engine-chosen port; `layerfs-sandbox-backend`'s
/// `backend/docker/create.rs`), and its recorded endpoint is that address.
const FORWARD: &str = "each docker Sandbox publishes its daemon's port on the host's loopback when it is created, and that published address is its endpoint";

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
        dial(
            spec,
            url,
            Loopback::Alias(layerfs_sandbox_backend::docker::DESKTOP_HOST),
        )
    }
    fn forward(&self, spec: &Spec, _guest_port: u16) -> Result<Forward, MachineError> {
        Err(refuse(spec, Code::InvalidInput, FORWARD))
    }
}
