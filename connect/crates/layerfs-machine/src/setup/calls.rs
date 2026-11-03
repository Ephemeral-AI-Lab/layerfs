//! The machine calls: create, connect, start, stop, remove, status, reach
//! and forward, each dispatched to the platform's adapter (`crate::adapter`).
//! Setup steps 1 to 4 (VM and disk, bundle, worker, forward and readiness)
//! end in a [`Running`]; writing `worker.json` and `record.json` is the
//! caller's step 5.
use super::guest::Guest;
use super::worker;
use crate::adapter::{adapter, refused};
use crate::error::{Code, MachineError};
use crate::platform::{HostOs, How, Route};
use crate::spec::{Bundle, Forward, Running, Spec};
use std::path::Path;
use std::time::Instant;

/// A machine's state as its provider reports it now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Running,
    Stopped,
    /// The provider reports the VM broken, or a failed create kept it.
    Failed,
    /// The provider holds no VM of that name.
    Missing,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
            Self::Missing => "missing",
        }
    }
}

/// `machine.status` (O34): state, free disk, the provider tool's version and
/// why the machine is down. A field a kind cannot report is `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub state: State,
    /// Free bytes of the data disk at `/layerfs/data`, as the guest sees them.
    pub free_disk: Option<u64>,
    pub tool_version: Option<String>,
    pub down_reason: Option<String>,
}

/// Makes a new VM machine and starts its worker. A create that fails before
/// the VM exists removes what it made; once it exists, the error has
/// `vm_exists` and the VM is kept for inspection. `Unknown` keeps everything.
pub fn create(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    spec.validate()?;
    worker::check_bundle(bundle)?;
    if spec.kind.profile().how == How::Connected {
        return Err(refused(spec, "is connected, not created"));
    }
    adapter(spec.kind).create(spec, bundle)
}

/// Connects an ssh host: checks it, copies the bundle in, starts the worker
/// and its forward. Nothing is created on the host but `/layerfs/bundle`,
/// `/layerfs/state` and the data root's folders.
pub fn connect(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    spec.validate()?;
    worker::check_bundle(bundle)?;
    if spec.kind.profile().how != How::Connected {
        return Err(refused(spec, "is created, not connected"));
    }
    adapter(spec.kind).create(spec, bundle)
}

/// Starts a stopped machine (or only its worker, where its kernel still
/// runs) and waits until the worker is ready. `None`: the machine's calls
/// run in the caller's process and there is nothing to start.
pub fn start(spec: &Spec, bundle: &Bundle) -> Result<Option<Running>, MachineError> {
    spec.validate()?;
    if spec.kind.route(HostOs::current()) == Route::InProcess {
        return Ok(None);
    }
    worker::check_bundle(bundle)?;
    adapter(spec.kind).start(spec, bundle)
}

/// Stops the machine's worker and, where it has one, its VM; disks stay.
/// `docker` stops only its worker, `ssh` its worker and forward; `native`
/// has nothing to stop.
pub fn stop(spec: &Spec) -> Result<(), MachineError> {
    spec.validate()?;
    adapter(spec.kind).stop(spec)
}

/// `machine remove` deletes a VM and its data disk; for `ssh` it is
/// `disconnect`: the worker and forward stop and `/layerfs/bundle` and
/// `/layerfs/state` go, never the data root. `InUse` is the caller's check.
pub fn remove(spec: &Spec) -> Result<(), MachineError> {
    spec.validate()?;
    adapter(spec.kind).remove(spec)
}

/// The machine's state now; it starts nothing.
pub fn status(spec: &Spec) -> Result<Status, MachineError> {
    spec.validate()?;
    adapter(spec.kind).status(spec)
}

/// [`status`], with every command it runs ending by `deadline`: one still
/// running then is killed with its process group and reaped, and the answer
/// is that command's `Unknown`. For callers that wait for a probe a bounded
/// time (`machine.list`), so a probe past it leaves no process behind.
pub fn status_within(spec: &Spec, deadline: Instant) -> Result<Status, MachineError> {
    super::tool::within(deadline, || status(spec))
}

/// Loads the docker-archive `archive` into the `docker` machine's Engine
/// and answers the image id Docker reports (#535 P5): what `machine start`
/// and `sandbox create` use on `docker` when no image is named. Refused for
/// any other platform.
pub fn load_image(spec: &Spec, archive: &Path) -> Result<String, MachineError> {
    spec.validate()?;
    if spec.kind != crate::platform::Platform::Docker {
        return Err(refused(spec, "loads no Docker image"));
    }
    crate::platforms::docker::load_image(spec, archive)
}

/// The identity of an ssh machine's host: its host key's fingerprint, else
/// its `/etc/machine-id`. A second machine on the same host is the caller's
/// `Exists`.
pub fn host_identity(spec: &Spec) -> Result<String, MachineError> {
    spec.validate()?;
    adapter(spec.kind).host_identity(spec)
}

/// The PostgreSQL Store URL a Sandbox daemon on `spec`'s machine dials for
/// `url` (#535 §7.3): `native` uses it as given; a loopback host becomes the
/// machine's host alias (`docker`: `host.docker.internal`, `lima` and
/// `firecracker`: `192.168.5.2`, `microsandbox`: `host.microsandbox.internal`,
/// `apple-container`: its network's gateway) or is refused (`ssh`); G7
/// receipts; a rewrite is refused when the URL asks for
/// `sslmode=verify-full`. Every other host is used as given. Starts nothing;
/// no message holds any part of the URL.
pub fn reach(spec: &Spec, url: &str) -> Result<String, MachineError> {
    spec.validate()?;
    adapter(spec.kind).reach(spec, url)
}

/// The host loopback address at which this process reaches a Sandbox
/// daemon listening on `127.0.0.1:<guest_port>` in `spec`'s machine (#535
/// G4, §7.2): Workspace calls dial it, not the worker. `native` answers
/// `127.0.0.1:<guest_port>` with nothing to close; `ssh` and `lima` start a
/// detached `ssh -N -L 127.0.0.1:<free port>:127.0.0.1:<guest_port>` (lima
/// through its own `ssh.config`, as its template forwards no other guest
/// port) and answer its pid, which [`forward_close`] ends. `docker` refuses
/// `InvalidInput`: each docker Sandbox publishes its daemon's port on the
/// host's loopback itself, and that is its endpoint. `apple-container`,
/// `microsandbox` (ports are published only at create) and `firecracker`
/// (its daemons listen on the microVM's loopback, which its Lima host's
/// forward cannot reach) refuse `Unsupported`; their Workspace calls stay
/// on the worker. One attempt: a forward that ends at once is `Failed`.
pub fn forward(spec: &Spec, guest_port: u16) -> Result<Forward, MachineError> {
    spec.validate()?;
    if guest_port == 0 {
        return Err(MachineError::new(
            Code::InvalidInput,
            "port 0 is no Sandbox daemon's port",
        ));
    }
    adapter(spec.kind).forward(spec, guest_port)
}

/// Ends the `ssh -L` a [`forward`] started, if it still runs and is still
/// that forward (a reused pid is never ended); a forward without one has
/// nothing to end. Bounded: SIGTERM, then SIGKILL after 3 s.
pub fn forward_close(spec: &Spec, forward: &Forward) {
    if let Some(pid) = forward.pid {
        super::forward::end(spec, pid, forward.address.port());
    }
}

/// Streams `entry` of the host folder `dir` (or all of it) into the guest
/// folder `destination` of a running machine, as one `tar` stream.
pub fn copy_in(
    spec: &Spec,
    dir: &Path,
    entry: Option<&str>,
    destination: &str,
) -> Result<(), MachineError> {
    spec.validate()?;
    guest_of(spec)?.copy_in(&spec.tools, dir, entry, None, destination)
}

/// Streams one guest file of a running machine to a new 0600 host file.
pub fn copy_out(spec: &Spec, source: &str, file: &Path) -> Result<(), MachineError> {
    spec.validate()?;
    guest_of(spec)?.copy_out(&spec.tools, source, file)
}

/// How the host reaches a running machine's guest; an in-process machine
/// has none (its files are the host's own).
fn guest_of(spec: &Spec) -> Result<Guest, MachineError> {
    adapter(spec.kind).guest(spec)
}

/// The status of a machine whose worker the host reaches through `spec.worker`:
/// a kernel that runs with a worker that does not answer reads as stopped.
pub(crate) fn with_worker(
    spec: &Spec,
    kernel: State,
    free_disk: Option<u64>,
    tool_version: Option<String>,
) -> Status {
    let (state, down_reason) = match kernel {
        State::Running if worker::answers(spec.worker.as_ref()) => (State::Running, None),
        State::Running => (
            State::Stopped,
            Some("the machine runs but its worker does not answer".to_owned()),
        ),
        State::Stopped => (State::Stopped, Some("the machine is stopped".to_owned())),
        State::Failed => (
            State::Failed,
            Some("the provider reports it broken".to_owned()),
        ),
        State::Missing => (
            State::Missing,
            Some("the provider holds no such VM".to_owned()),
        ),
    };
    Status {
        state,
        free_disk,
        tool_version,
        down_reason,
    }
}
