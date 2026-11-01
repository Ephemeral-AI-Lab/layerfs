//! A machine starts on the first call that needs it (H §7.2, step 13): the
//! caller finds the worker gone, starts the machine and its worker, waits
//! until it is ready and replaces `worker.json`. That wait is a readiness
//! wait, not a retry; the call itself is then sent once. Two processes that
//! need the same stopped machine take turns on its `start.lock`.
use super::record::{self, machine_of, refusal, save_running};
use crate::host::outcome::{failed, refused};
use crate::worker::Endpoint;
use layerfs_bridge::control::SdkRefusal;
use layerfs_sandbox::machine::{self as adapters, HostOs, Route};
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
    let lock_path = crate::home::paths::machine_dir(home, name).join("start.lock");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&lock_path)
        .map_err(failed)?;
    let deadline = Instant::now() + START_WAIT;
    while !crate::host::flock::try_lock(&file, true).map_err(failed)? {
        if Instant::now() >= deadline {
            return Err(refused(
                "Unavailable",
                format!("machine {name} is being started by another process"),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let started = (|| {
        // another process may have started it meanwhile
        if let Some(endpoint) = answering(home, name)? {
            return Ok(Some(endpoint));
        }
        let id = crate::home::open(home)?.id;
        let spec = record::spec(home, &id, name, &record)?;
        let bundle = super::bundle::bundle()?;
        match adapters::start(&spec, &bundle).map_err(refusal)? {
            None => Ok(None),
            Some(running) => {
                let _lock = crate::home::lock::take(home)?;
                save_running(home, name, &running)?;
                Ok(Some(endpoint_of(&running.endpoint)))
            }
        }
    })();
    crate::host::flock::unlock(&file);
    started
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
    Ok(crate::worker::ping(&endpoint).is_ok().then_some(endpoint))
}
