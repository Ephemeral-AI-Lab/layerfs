//! A running Sandbox daemon's control-capacity read model (#540): its
//! `backend.daemon.connection_failures` answer, read once, best-effort and
//! bounded. The doctor explains it and `sandbox.get` shows it as
//! `daemon_capacity`; both ask through here so one place decides where the
//! daemon is, how long an answer may take, and what an unanswered query is.
//!
//! Both reads end at [`WAIT`], and their connection is fenced at that
//! deadline, so a daemon that trickles its handshake or its one reply keeps
//! neither a thread, a connection nor a control slot past it.
use crate::daemon::fence::Fence;
use crate::daemon::pool::{Controls, Dial};
use crate::daemon::{operator, trace};
use crate::direct;
use crate::internal::Control;
use crate::record::SandboxRecord;
use layerfs_bridge::control::{CapacityModel, Request, SdkRefusal, SdkValue};
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, refused};
use layerfs_home::secrets;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The daemon's operator read model of control admission and retained
/// connection failures.
pub const OPERATION: &str = layerfs_bridge::control::OPERATION;

/// How long one daemon query may take: connecting, its Hello and its one
/// exchange. Short on purpose: a reader must not stall behind a daemon that
/// is unreachable, as an ordinary 60 s admission wait would.
pub const WAIT: Duration = Duration::from_secs(2);

/// The model [`read`] gives: the daemon's answer, parsed once.
pub fn read(
    home: &Path,
    sandbox: &SandboxRecord,
    endpoint: SocketAddr,
) -> Result<CapacityModel, SdkRefusal> {
    let home = home.to_owned();
    let sandbox = sandbox.clone();
    bounded(WAIT, move || query(&home, &sandbox, endpoint))
        .map(|answer| CapacityModel::parse(&answer))
}

/// The daemon's answer to [`OPERATION`] on a connection the Sandbox's pool
/// lends (`sandbox.get`, #543): at most [`WAIT`] for admission and for the
/// exchange, which is fenced at the same deadline, so this read takes no
/// second connection and no second slot of its own. A window the admission
/// wait used up refuses the read before it is sent, and a connection the
/// fence ends stays quarantined: the pool closes it instead of lending it
/// again. Nothing is sent again.
pub fn lent(controls: &Controls) -> Result<SdkValue, SdkRefusal> {
    let deadline = Instant::now() + WAIT;
    let mut control = trace::control_within(controls, WAIT)?;
    if deadline.saturating_duration_since(Instant::now()).is_zero() {
        // the admission wait used the whole window. The connection is healthy
        // and returns to the pool; arming a fence for a deadline that already
        // passed would shut it down for nothing, so nothing is sent.
        return Err(refused(
            "Unavailable",
            format!(
                "no control connection came free within {}s, so the capacity read was not sent",
                WAIT.as_secs()
            ),
        ));
    }
    let fence = Fence::channel((control.close_handle().map_err(failed))?, deadline);
    let answer = exchange(&mut control);
    drop(fence);
    answer
}

/// The address of a running Sandbox's daemon when its record names one to
/// this host: a forwarded machine's forward, else the endpoint the start
/// recorded in `control.json`. Nothing is observed or started.
pub fn endpoint(home: &Path, sandbox: &SandboxRecord) -> Option<SocketAddr> {
    if let Some(forward) = sandbox.daemon().and_then(direct::forward_of) {
        return Some(forward);
    }
    let keys = secrets::daemon_keys(&paths::sandbox_dir(home, sandbox.name()))
        .ok()
        .flatten()?;
    keys.get_text("endpoint")?.parse().ok()
}

/// [`query`] under a hard wall: a daemon that accepts a connection and then
/// says nothing must not hold the caller past [`WAIT`], even where the fence
/// itself cannot be armed.
fn bounded<T: Send + 'static>(
    within: Duration,
    work: impl FnOnce() -> Result<T, SdkRefusal> + Send + 'static,
) -> Result<T, SdkRefusal> {
    let (sent, received) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sent.send(work());
    });
    match received.recv_timeout(within) {
        Ok(result) => result,
        Err(_) => Err(refused(
            "Unavailable",
            format!("the daemon did not answer within {}s", within.as_secs()),
        )),
    }
}

/// One daemon query on a connection dialed for it: its recorded endpoint with
/// its keys, for at most [`WAIT`], fenced at the same deadline, then read the
/// capacity model once.
fn query(
    home: &Path,
    sandbox: &SandboxRecord,
    endpoint: SocketAddr,
) -> Result<SdkValue, SdkRefusal> {
    let keys = secrets::daemon_keys(&paths::sandbox_dir(home, sandbox.name()))?
        .ok_or_else(|| refused("NoSuchSandbox", "the Sandbox has no daemon keys"))?;
    let dial = Dial {
        endpoint,
        secret: secrets::key(&keys, "secret")?,
        peer: secrets::key(&keys, "peer")?,
        instance: secrets::key(&keys, "instance")?,
    };
    let (mut control, fence) = dial.open_within(WAIT)?;
    let answer = exchange(&mut control);
    // this conversation ends here: a connection dropped instead leaves the
    // daemon reading a closed socket, which it logs as an unexpected end
    let _ = control.call(Request::EndSession);
    drop(fence);
    answer
}

/// The one exchange: the daemon's answer, or the refusal it gave, kept exact.
fn exchange(control: &mut Control) -> Result<SdkValue, SdkRefusal> {
    operator::call(control, OPERATION, SdkValue::Null).map_err(|error| match error {
        crate::SdkError::Refused(refusal) => refusal,
        crate::SdkError::Operation(failure) => {
            crate::daemon::values::refusal(&failure).unwrap_or_else(|| failed(failure))
        }
        crate::SdkError::Mount(failure) => failed(failure),
    })
}
