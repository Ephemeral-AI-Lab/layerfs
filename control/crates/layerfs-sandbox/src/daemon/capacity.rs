//! A running Sandbox daemon's control-capacity read model (#540): its
//! `backend.daemon.connection_failures` answer, read once, best-effort and
//! bounded. The doctor explains it and `sandbox.get` shows it as
//! `daemon_capacity`; both ask through here so one place decides where the
//! daemon is, how long an answer may take, and what an unanswered query is.
use crate::daemon::{operator, pool::Dial};
use crate::direct;
use crate::record::SandboxRecord;
use layerfs_bridge::control::{CapacityModel, SdkRefusal};
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, refused};
use layerfs_home::secrets;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// The daemon's operator read model of control admission and retained
/// connection failures.
pub const OPERATION: &str = layerfs_bridge::control::CAPACITY_OPERATION;

/// How long one daemon query may take: connecting, its Hello and its one
/// exchange. Short on purpose: a reader must not stall behind a daemon that
/// is unreachable, as an ordinary 60 s admission wait would.
pub const WAIT: Duration = Duration::from_secs(2);

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
/// says nothing must not hold the caller past [`WAIT`].
pub fn read(
    home: &Path,
    sandbox: &SandboxRecord,
    endpoint: SocketAddr,
) -> Result<CapacityModel, SdkRefusal> {
    let home = home.to_owned();
    let sandbox = sandbox.clone();
    let (sent, received) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sent.send(query(&home, &sandbox, endpoint));
    });
    match received.recv_timeout(WAIT) {
        Ok(result) => result.map(|value| CapacityModel::parse(&value)),
        Err(_) => Err(refused(
            "Unavailable",
            format!("the daemon did not answer within {}s", WAIT.as_secs()),
        )),
    }
}

/// One daemon query: dial its recorded endpoint with its keys for at most
/// [`WAIT`], then read the capacity model once.
fn query(
    home: &Path,
    sandbox: &SandboxRecord,
    endpoint: SocketAddr,
) -> Result<layerfs_bridge::control::SdkValue, SdkRefusal> {
    let keys = secrets::daemon_keys(&paths::sandbox_dir(home, sandbox.name()))?
        .ok_or_else(|| refused("NoSuchSandbox", "the Sandbox has no daemon keys"))?;
    let dial = Dial {
        endpoint,
        secret: secrets::key(&keys, "secret")?,
        peer: secrets::key(&keys, "peer")?,
        instance: secrets::key(&keys, "instance")?,
    };
    let mut control = dial.open_within(WAIT)?;
    operator::call(
        &mut control,
        OPERATION,
        layerfs_bridge::control::SdkValue::Null,
    )
    .map_err(|error| match error {
        crate::SdkError::Refused(refusal) => refusal,
        crate::SdkError::Operation(failure) => {
            crate::daemon::values::refusal(&failure).unwrap_or_else(|| failed(failure))
        }
        crate::SdkError::Mount(failure) => failed(failure),
    })
}
