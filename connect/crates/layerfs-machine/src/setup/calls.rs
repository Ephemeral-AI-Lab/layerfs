//! The machine calls: create, connect, start, stop, remove and status, each
//! dispatched once on the machine's kind to its adapter. Setup steps 1 to 4
//! (VM and disk, bundle, worker, forward and readiness) end in a [`Running`];
//! writing `worker.json` and `record.json` is the caller's step 5.
use super::guest::Guest;
use super::worker;
use crate::error::{Code, MachineError};
use crate::kind::{HostOs, Kind, Route};
use crate::platforms::{self as vm, docker, native};
use crate::spec::{Bundle, Running, Spec};
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
    match spec.kind {
        Kind::AppleContainer => vm::apple_container::create(spec, bundle),
        Kind::Microsandbox => vm::microsandbox::create(spec, bundle),
        Kind::Lima => vm::lima::create(spec, bundle),
        Kind::Firecracker => vm::firecracker::create(spec, bundle),
        Kind::Ssh => Err(refused(spec, "is connected, not created")),
        Kind::Native | Kind::Docker => Err(refused(spec, "is built in and never created")),
    }
}

/// Connects an ssh host: checks it, copies the bundle in, starts the worker
/// and its forward. Nothing is created on the host but `/layerfs/bundle`,
/// `/layerfs/state` and the data root's folders.
pub fn connect(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    spec.validate()?;
    worker::check_bundle(bundle)?;
    match spec.kind {
        Kind::Ssh => vm::ssh::connect(spec, bundle),
        _ => Err(refused(spec, "is created, not connected")),
    }
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
    match spec.kind {
        Kind::Native => native::start(spec),
        Kind::Docker => docker::start(spec, bundle).map(Some),
        Kind::AppleContainer => vm::apple_container::start(spec, bundle).map(Some),
        Kind::Microsandbox => vm::microsandbox::start(spec, bundle).map(Some),
        Kind::Lima => vm::lima::start(spec, bundle).map(Some),
        Kind::Firecracker => vm::firecracker::start(spec, bundle).map(Some),
        Kind::Ssh => vm::ssh::start(spec, bundle).map(Some),
    }
}

/// Stops the machine's worker and, where it has one, its VM; disks stay.
/// `docker` stops only its worker, `ssh` its worker and forward; `native`
/// has nothing to stop.
pub fn stop(spec: &Spec) -> Result<(), MachineError> {
    spec.validate()?;
    match spec.kind {
        Kind::Native => Ok(()),
        Kind::Docker => docker::stop(spec),
        Kind::AppleContainer => vm::apple_container::stop(spec),
        Kind::Microsandbox => vm::microsandbox::stop(spec),
        Kind::Lima => vm::lima::stop(spec),
        Kind::Firecracker => vm::firecracker::stop(spec),
        Kind::Ssh => vm::ssh::stop(spec),
    }
}

/// `machine remove` deletes a VM and its data disk; for `ssh` it is
/// `disconnect`: the worker and forward stop and `/layerfs/bundle` and
/// `/layerfs/state` go, never the data root. `InUse` is the caller's check.
pub fn remove(spec: &Spec) -> Result<(), MachineError> {
    spec.validate()?;
    match spec.kind {
        Kind::AppleContainer => vm::apple_container::remove(spec),
        Kind::Microsandbox => vm::microsandbox::remove(spec),
        Kind::Lima => vm::lima::remove(spec),
        Kind::Firecracker => vm::firecracker::remove(spec),
        Kind::Ssh => vm::ssh::disconnect(spec),
        Kind::Native | Kind::Docker => Err(refused(spec, "is built in and never removed")),
    }
}

/// The machine's state now; it starts nothing.
pub fn status(spec: &Spec) -> Result<Status, MachineError> {
    spec.validate()?;
    match spec.kind {
        Kind::Native => Ok(native::status()),
        Kind::Docker => docker::status(spec),
        Kind::AppleContainer => vm::apple_container::status(spec),
        Kind::Microsandbox => vm::microsandbox::status(spec),
        Kind::Lima => vm::lima::status(spec),
        Kind::Firecracker => vm::firecracker::status(spec),
        Kind::Ssh => vm::ssh::status(spec),
    }
}

/// [`status`], with every command it runs ending by `deadline`: one still
/// running then is killed with its process group and reaped, and the answer
/// is that command's `Unknown`. For callers that wait for a probe a bounded
/// time (`machine.list`), so a probe past it leaves no process behind.
pub fn status_within(spec: &Spec, deadline: Instant) -> Result<Status, MachineError> {
    super::tool::within(deadline, || status(spec))
}

/// The identity of an ssh machine's host: its host key's fingerprint, else
/// its `/etc/machine-id`. A second machine on the same host is the caller's
/// `Exists`.
pub fn host_identity(spec: &Spec) -> Result<String, MachineError> {
    spec.validate()?;
    match spec.kind {
        Kind::Ssh => vm::ssh::identity(spec),
        _ => Err(refused(spec, "has no ssh host")),
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
    match spec.kind {
        Kind::AppleContainer => Ok(vm::apple_container::guest(spec)),
        Kind::Microsandbox => Ok(vm::microsandbox::guest(spec)),
        Kind::Lima => vm::lima::guest(spec),
        Kind::Firecracker => vm::firecracker::machine_guest(spec),
        Kind::Ssh => vm::ssh::machine_guest(spec),
        Kind::Docker if Kind::Docker.route(HostOs::current()) == Route::Worker => {
            docker::guest(spec)
        }
        Kind::Native | Kind::Docker => {
            Err(refused(spec, "runs in-process; its files are the host's"))
        }
    }
}

fn refused(spec: &Spec, why: &str) -> MachineError {
    MachineError::new(
        Code::InvalidInput,
        format!("the {} machine {} {why}", spec.kind.name(), spec.name),
    )
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
