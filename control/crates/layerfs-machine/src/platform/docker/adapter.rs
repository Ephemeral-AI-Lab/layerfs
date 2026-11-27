//! `docker`'s adapter: the built-in machine on Docker's kernel, whose worker
//! container runs `namespaces` Sandboxes (D-8).
use super::{guest, start, status, stop};
use crate::platform::adapter::{refused, Adapter};
use crate::platform::docker::socket::{socket_state, SocketState, SOCKET_ANSWER};
use crate::platform::error::{Code, MachineError};
use crate::setup::forward::refuse;
use crate::setup::guest::Guest;
use crate::setup::probe::first_line;
use crate::setup::spec::{Bundle, Forward, NameLimit, Running, Spec, Tools};
use crate::setup::{tool, Status};
use crate::table::profile::{ExecutorKind, HostOs, Platform};
use crate::table::reach::Loopback;

/// Why a Sandbox daemon's port is not forwarded: the worker container's
/// port is published when it starts (`docker run -p`), which is the only
/// way Docker publishes one, so a later daemon's port cannot be (D-8).
const FORWARD: &str = "Docker publishes a container's ports only when it starts, so a Sandbox daemon's port in the worker container cannot be published later; its Workspace calls go through the worker";

/// The name Docker Desktop's DNS gives this host in its containers.
const DESKTOP_HOST: &str = "host.docker.internal";

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
        // the Engine is asked for its version only once its socket answers a
        // ping (bounded), so a silent socket costs no `docker` call
        let socket = &tools.docker_socket;
        let version = match socket_state(socket, SOCKET_ANSWER) {
            SocketState::Answering => {
                let mut command = tool::command(tools, &docker, ["-H"]);
                command.arg(format!("unix://{}", socket.display())).args([
                    "version",
                    "--format",
                    "{{.Server.Version}}",
                ]);
                first_line(command, false)
            }
            _ => None,
        };
        if version.is_none() {
            missing.push(format!("host:{}", socket.display()));
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
    fn name_limit(&self) -> Option<NameLimit> {
        None
    }
    fn data_volume(&self, spec: &Spec) -> Option<String> {
        Some(crate::setup::vm::data_volume(spec))
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
    fn stop(&self, spec: &Spec, force: bool) -> Result<(), MachineError> {
        stop(spec, force)
    }
    fn provider_state(&self, spec: &Spec) -> Result<crate::setup::State, MachineError> {
        super::provider_state(spec)
    }
    fn status(&self, spec: &Spec) -> Result<Status, MachineError> {
        status(spec)
    }
    /// On a Linux Engine the worker shares the host's network, and so do
    /// its `namespaces` Sandboxes' daemons, which dial the Store (their
    /// commands have a network of their own), so loopback is shared. Docker Desktop's worker has a network of its own, which
    /// calls this host `host.docker.internal` (Docker Desktop's DNS, which
    /// the worker's Sandboxes use too).
    fn loopback(&self, _spec: &Spec) -> Loopback {
        if cfg!(target_os = "linux") {
            return Loopback::Shared;
        }
        Loopback::Apart(format!("{DESKTOP_HOST} names this host there"))
    }
    fn forward(&self, spec: &Spec, _guest_port: u16) -> Result<Forward, MachineError> {
        Err(refuse(spec, Code::Unsupported, FORWARD))
    }
}
