//! A machine starts on the first call that needs it (H §7.2, step 13): the
//! caller finds the worker gone, starts the machine and its worker, waits
//! until it is ready and replaces `worker.json`. That wait is a readiness
//! wait, not a retry; the call itself is then sent once. Two processes that
//! need the same stopped machine take turns on its `start.lock`. A
//! Firecracker machine's Lima host is started the same way first: its own
//! worker may have stopped it.
use super::record::{self, machine_of, refusal, save_running};
use crate::common::outcome::{failed, refused};
use crate::machine::worker::Endpoint;
use layerfs_bridge::control::SdkRefusal;
use layerfs_machine::{self as adapters, HostOs, Platform, Route};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long a call waits for another process starting the same machine.
const START_WAIT: Duration = Duration::from_secs(600);

/// The worker of machine `name`, started if it is not running; `None` when
/// the machine's calls run in this process (`native`, `docker` on Linux).
pub fn running(home: &Path, name: &str) -> Result<Option<Endpoint>, SdkRefusal> {
    let record = machine_of(home, name)?;
    let kind = record::kind_of(&record)?;
    if kind.route(HostOs::current()) == Route::InProcess {
        return Ok(None);
    }
    if let Some(endpoint) = answering(home, name)? {
        return Ok(Some(endpoint));
    }
    if kind == Platform::Firecracker {
        if let Some(host) = record.get_text("host") {
            running(home, host)?;
        }
    }
    let _start = start_lock(home, name)?;
    (|| {
        // another process may have started it, or removed it, meanwhile
        let record = machine_of(home, name)?;
        if let Some(endpoint) = answering(home, name)? {
            return Ok(Some(endpoint));
        }
        let id = crate::common::home::open(home)?.id;
        let spec = record::spec(home, &id, name, &record)?;
        let bundle = super::bundle::bundle()?;
        match adapters::start(&spec, &bundle).map_err(refusal)? {
            None => Ok(None),
            Some(running) => {
                let _lock = crate::common::home::lock::take(home)?;
                save_running(home, name, &running)?;
                Ok(Some(endpoint_of(&running.endpoint)))
            }
        }
    })()
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
