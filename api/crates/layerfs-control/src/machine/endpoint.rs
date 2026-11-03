//! A machine starts on the first call that needs it (H §7.2, step 13). The
//! call dials the worker its `worker.json` records, once; only when nothing
//! was sent (the connection, the secret or its answer failed) does the
//! caller start the machine and its worker, wait until it is ready, replace
//! `worker.json` and send the call once (#535 R1, [`start_after`]). That
//! wait is a readiness wait, not a retry: the first attempt never reached
//! the worker. `machine.start` and an operation of several steps make the
//! machine ready first instead ([`running`]). Two processes that need the
//! same stopped machine take turns on its `start.lock`. A Firecracker
//! machine's Lima host is started the same way first: its own worker may
//! have stopped it.
use super::record::{self, machine_of, refusal, save_running};
use crate::common::outcome::{failed, refused};
use crate::machine::worker::Endpoint;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_machine::{self as adapters, HostOs, Platform, Route};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long a call waits for another process starting the same machine.
const START_WAIT: Duration = Duration::from_secs(600);

/// The worker of machine `name`, started if it does not answer its secret;
/// `None` when the machine's calls run in this process (`native`).
pub fn running(home: &Path, name: &str) -> Result<Option<Endpoint>, SdkRefusal> {
    let record = machine_of(home, name)?;
    let kind = record::kind_of(&record)?;
    if kind.route(HostOs::current()) == Route::InProcess {
        return Ok(None);
    }
    if let Some(endpoint) = answering(home, name)? {
        return Ok(Some(endpoint));
    }
    host_first(home, &record)?;
    let _start = start_lock(home, name)?;
    // another process may have started it, or removed it, meanwhile
    let record = machine_of(home, name)?;
    if let Some(endpoint) = answering(home, name)? {
        return Ok(Some(endpoint));
    }
    start(home, name, &record)
}

/// Starts machine `name` and its worker after a call to the worker it
/// recorded (`tried`, or none) was not sent (R1), and answers the worker to
/// send that call to, once. A worker another process recorded meanwhile is
/// that one, as recorded: it is not pinged first.
pub fn start_after(
    home: &Path,
    name: &str,
    tried: Option<&Endpoint>,
) -> Result<Endpoint, SdkRefusal> {
    let record = machine_of(home, name)?;
    host_first(home, &record)?;
    let _start = start_lock(home, name)?;
    let record = machine_of(home, name)?;
    if let Some(now) = record::worker(home, name)? {
        let now = endpoint_of(&now);
        if Some(&now) != tried {
            return Ok(now);
        }
    }
    start(home, name, &record)?
        .ok_or_else(|| refused("Failed", format!("machine {name} has no worker to start")))
}

/// A Firecracker machine's Lima host, started first if it does not answer.
fn host_first(home: &Path, record: &SdkValue) -> Result<(), SdkRefusal> {
    if record::kind_of(record)? == Platform::Firecracker {
        if let Some(host) = record.get_text("host") {
            running(home, host)?;
        }
    }
    Ok(())
}

/// Starts machine `name` (its `record`) and records its worker; under its
/// start lock.
fn start(home: &Path, name: &str, record: &SdkValue) -> Result<Option<Endpoint>, SdkRefusal> {
    let id = crate::common::home::open(home)?.id;
    let spec = record::spec(home, &id, name, record)?;
    let bundle = super::bundle::bundle()?;
    match adapters::start(&spec, &bundle).map_err(refusal)? {
        None => Ok(None),
        Some(running) => {
            let _lock = crate::common::home::lock::take(home)?;
            save_running(home, name, &running)?;
            Ok(Some(endpoint_of(&running.endpoint)))
        }
    }
}

/// Machine `name`'s `start.lock`, held until dropped. Starting, creating and
/// removing a machine take it, always before the home lock, so an
/// on-demand start never runs beside a create or a remove of the same VM.
/// A wait for it, at most [`START_WAIT`], is not a retry.
pub(crate) struct StartLock(std::fs::File);

impl Drop for StartLock {
    fn drop(&mut self) {
        crate::common::home::flock::unlock(&self.0);
    }
}

pub(crate) fn start_lock(home: &Path, name: &str) -> Result<StartLock, SdkRefusal> {
    let lock_path = crate::common::home::paths::machine_dir(home, name).join("start.lock");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&lock_path)
        .map_err(failed)?;
    let deadline = Instant::now() + START_WAIT;
    while !crate::common::home::flock::try_lock(&file, true).map_err(failed)? {
        if Instant::now() >= deadline {
            return Err(refused(
                "Unavailable",
                format!("machine {name} is being started, made or removed by another process"),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(StartLock(file))
}

fn endpoint_of(endpoint: &adapters::Endpoint) -> Endpoint {
    Endpoint {
        address: endpoint.address,
        secret: endpoint.secret.clone(),
    }
}

/// The recorded worker, when it answers its secret now.
fn answering(home: &Path, name: &str) -> Result<Option<Endpoint>, SdkRefusal> {
    let Some(recorded) = record::worker(home, name)? else {
        return Ok(None);
    };
    let endpoint = endpoint_of(&recorded);
    Ok(crate::machine::worker::ping(&endpoint)
        .is_ok()
        .then_some(endpoint))
}
