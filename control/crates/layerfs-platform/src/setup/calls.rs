//! The machine calls: create, start, stop, remove, status, reach
//! and forward, each dispatched to the platform's adapter (`crate::adapter`).
//! Setup steps 1 to 4 (VM and disk, bundle, worker, forward and readiness)
//! end in a [`Running`]; writing `worker.json` and `record.json` is the
//! caller's step 5.
use super::guest::Guest;
use super::worker;
use crate::adapter::{adapter, refused};
use crate::error::{Code, MachineError};
use crate::platform::{HostOs, Platform, Route};
use crate::spec::{Bundle, Forward, NameLimit, Running, Spec};
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

/// A machine's status now (O34): what its provider reports, whether its
/// worker answered, free disk, the provider tool's version and why the
/// machine is down. A field a kind cannot report is `None`. [`Status::word`]
/// and [`Status::reason`] say it in the call surface's state words
/// (calls.json machine states), as `machine get --with live` does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// What the provider reports of the VM or container itself; `Stopped`
    /// when the provider did not answer (`kernel` is then `None`). A VM that
    /// runs is `Running` here whether or not its worker answers.
    pub state: State,
    /// What the provider said, `None` when it did not answer (the Docker
    /// Engine is not running).
    pub kernel: Option<State>,
    /// Whether the worker at the spec's endpoint answered its hello, asked
    /// only when the kernel runs: `Some(false)` when no endpoint was given.
    /// `None` when not asked, and on a machine whose calls run in the
    /// caller's process (it has no worker).
    pub worker: Option<bool>,
    /// Free bytes of the data disk at `/layerfs/data`, as the guest sees them.
    pub free_disk: Option<u64>,
    pub tool_version: Option<String>,
    pub down_reason: Option<String>,
}

impl Status {
    /// The state word: `broken` with a [`Status::reason`], `ready` when the
    /// kernel runs and no worker was found silent, else `stopped`.
    pub fn word(&self) -> &'static str {
        match (self.reason(), self.kernel) {
            (Some(_), _) => "broken",
            (None, Some(State::Running)) => "ready",
            (None, _) => "stopped",
        }
    }

    /// Why it is broken, as calls.json's reason code: `engine_down` (the
    /// provider did not answer), `missing`, `failed`, or `not_answering` (it
    /// runs and its worker does not answer). `None` when it is not broken.
    pub fn reason(&self) -> Option<&'static str> {
        match self.kernel {
            None => Some("engine_down"),
            Some(State::Missing) => Some("missing"),
            Some(State::Failed) => Some("failed"),
            Some(State::Running) if self.worker == Some(false) => Some("not_answering"),
            Some(State::Running | State::Stopped) => None,
        }
    }
}

/// The provider's own limit on the name of a `platform` VM on this host,
/// beyond the plain-name rule; `None` where it has none. Read from the
/// provider's settings: the provider is asked nothing and nothing is made.
pub fn name_limit(platform: Platform) -> Option<NameLimit> {
    adapter(platform).name_limit()
}

/// The provider's name of the volume or disk that holds the data of the
/// machine `spec` describes: a docker or Apple Container volume
/// (`<vm_name>-data`), a Lima disk; `None` on `native`, whose data is the
/// home's. From the spec alone: the provider is asked nothing.
pub fn data_volume(spec: &Spec) -> Option<String> {
    adapter(spec.kind).data_volume(spec)
}

/// Makes a new VM machine and starts its worker. A create that fails before
/// the VM exists removes what it made; once it exists, the error has
/// `vm_exists` and the VM is kept for inspection. `Unknown` keeps everything.
pub fn create(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    spec.validate()?;
    worker::check_bundle(bundle)?;
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
/// `docker` stops only its worker; `native` has nothing to stop.
pub fn stop(spec: &Spec) -> Result<(), MachineError> {
    spec.validate()?;
    adapter(spec.kind).stop(spec, false)
}

/// Stops directly through the provider, without contacting or starting its worker.
/// The caller owns Sandbox/Workspace reconciliation; registered machines and disks stay.
pub fn stop_force(spec: &Spec) -> Result<(), MachineError> {
    spec.validate()?;
    adapter(spec.kind).stop(spec, true)
}

/// `machine remove` deletes a VM and its data disk. `InUse` is the
/// caller's check.
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

/// Observes provider prerequisites, version and runtime state by `deadline`.
/// Unlike status, this never dials a worker or executes a guest command.
/// Capacity is not observed; caller must report it as unverified.
pub fn provider_within(spec: &Spec, deadline: Instant) -> Result<Status, MachineError> {
    spec.validate()?;
    if crate::support::support(spec.kind, HostOs::current())
        .backend
        .is_none()
    {
        return Err(MachineError::new(
            Code::Unsupported,
            "machine platform is unsupported on this host",
        ));
    }
    super::tool::within(deadline, || {
        let probe = crate::probe::probe(spec.kind, &spec.tools);
        if Instant::now() >= deadline {
            return Err(MachineError::new(
                Code::Unknown,
                "provider observation deadline exceeded",
            ));
        }
        if !probe.ready {
            return Err(MachineError::missing(
                "provider prerequisites are unavailable",
                probe.missing,
            ));
        }
        let state = adapter(spec.kind).provider_state(spec)?;
        Ok(Status {
            state,
            kernel: Some(state),
            worker: None,
            free_disk: None,
            tool_version: probe.version,
            down_reason: None,
        })
    })
}

/// Loads the docker-archive `archive` into the `docker` machine's Engine
/// and answers the image id Docker reports (#535 P5): what `machine start`
/// uses on `docker` for the worker container when no image is named. Refused for
/// any other platform.
pub fn load_image(spec: &Spec, archive: &Path) -> Result<String, MachineError> {
    spec.validate()?;
    if spec.kind != crate::platform::Platform::Docker {
        return Err(refused(spec, "loads no Docker image"));
    }
    crate::platforms::docker::load_image(spec, archive)
}

/// The PostgreSQL Store URL a Sandbox daemon on `spec`'s machine dials for
/// `url` (#535 §7.3): `native` uses it as given; a loopback host becomes the
/// machine's host alias (`docker`: on Docker Desktop the address its running
/// worker container resolves `host.docker.internal` to, on a Linux Engine
/// the URL as given; `lima`: `192.168.5.2`, `apple-container`: its
/// network's gateway;
/// G7 receipts); a rewrite is refused when the URL asks for
/// `sslmode=verify-full`. Every other host is used as given. Starts nothing;
/// no message holds any part of the URL.
pub fn reach(spec: &Spec, url: &str) -> Result<String, MachineError> {
    spec.validate()?;
    adapter(spec.kind).reach(spec, url)
}

/// The host loopback address at which this process reaches a Sandbox
/// daemon listening on `127.0.0.1:<guest_port>` in `spec`'s machine (#535
/// G4, §7.2): Workspace calls dial it, not the worker. `native` answers
/// `127.0.0.1:<guest_port>` with nothing to close; `lima` starts a
/// detached `ssh -N -L 127.0.0.1:<free port>:127.0.0.1:<guest_port>` (lima
/// through its own `ssh.config`, as its template forwards no other guest
/// port) and answer its pid, which [`forward_close`] ends. `docker` (its
/// worker container's ports are published only at start, D-8),
/// `apple-container` (ports are published only at create) refuse
/// `Unsupported`; their Workspace calls stay on the worker. One attempt: a forward that ends at once is `Failed`.
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
    guest_of(spec)?.copy_in(&spec.tools, dir, entry, destination)
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

/// The status of a machine whose worker the host reaches through
/// `spec.worker`, asked once when the kernel runs: a kernel that runs with a
/// worker that does not answer (or no endpoint) is `broken`
/// `not_answering`, never `stopped` (calls.json machine states).
pub(crate) fn with_worker(
    spec: &Spec,
    kernel: State,
    free_disk: Option<u64>,
    tool_version: Option<String>,
) -> Status {
    let worker = (kernel == State::Running).then(|| worker::answers(spec.worker.as_ref()));
    let down_reason = match (kernel, worker) {
        (State::Running, Some(true)) => None,
        (State::Running, _) => Some("the machine runs but its worker does not answer"),
        (State::Stopped, _) => Some("the machine is stopped"),
        (State::Failed, _) => Some("the provider reports it broken"),
        (State::Missing, _) => Some("the provider holds no such VM"),
    };
    Status {
        state: kernel,
        kernel: Some(kernel),
        worker,
        free_disk,
        tool_version,
        down_reason: down_reason.map(str::to_owned),
    }
}

/// Reuses the Engine's content-tagged Sandbox image, or imports it once.
/// `previous` is a recorded Docker ID known by the caller to hold this content.
/// Docker's actual imported ID is returned, never the archive config digest.
pub fn cached_image(
    spec: &Spec,
    archive: &Path,
    content: &str,
    previous: Option<&str>,
) -> Result<String, MachineError> {
    spec.validate()?;
    if spec.kind != crate::platform::Platform::Docker {
        return Err(refused(spec, "loads no Docker image"));
    }
    crate::platforms::docker::cached_image(spec, archive, content, previous)
}
