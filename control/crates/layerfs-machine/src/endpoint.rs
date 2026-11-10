//! `machine.start` (#535 D11: the only call that starts a machine): it
//! dials the worker its `worker.json` records and, when that worker does
//! not answer, starts the machine and its worker, waits until it is ready
//! (a readiness wait, not a retry) and replaces `worker.json`
//! ([`running`]). Two processes starting the same machine take turns on its
//! `start.lock`.
use super::record::MachineRecord;
use super::spec::{self as record, machine_of, refusal, save_running};
use crate::progress::stage;
use crate::worker::Endpoint;
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::events::Events;
use layerfs_platform::{self as adapters, HostOs, Platform, Route};
use std::path::Path;
use std::time::Duration;

/// How long a call waits for another process starting the same machine.
const START_WAIT: Duration = Duration::from_secs(600);

/// The worker of machine `name`, started if it does not answer its secret;
/// `None` when the machine's calls run in this process (`native`).
pub fn running(home: &Path, name: &str) -> Result<Option<Endpoint>, SdkRefusal> {
    let mut sink = |_| {};
    running_with_progress(home, name, &mut Events::new(&mut sink))
}

/// Starts or observes a worker while reporting stage boundaries and elapsed time.
pub fn running_with_progress(
    home: &Path,
    name: &str,
    events: &mut Events<'_>,
) -> Result<Option<Endpoint>, SdkRefusal> {
    let record = machine_of(home, name)?;
    let kind = record.platform()?;
    if kind.route(HostOs::current()) == Route::InProcess {
        return Ok(None);
    }
    if let Some(endpoint) = answering(home, name)? {
        return Ok(Some(endpoint));
    }
    let _start = start_lock(home, name)?;
    // another process may have started it, or removed it, meanwhile
    let record = machine_of(home, name)?;
    if let Some(endpoint) = answering(home, name)? {
        return Ok(Some(endpoint));
    }
    start(home, name, &record, events)
}

/// Starts machine `name` (its `record`) and records its worker; under its
/// start lock.
fn start(
    home: &Path,
    name: &str,
    record: &MachineRecord,
    events: &mut Events<'_>,
) -> Result<Option<Endpoint>, SdkRefusal> {
    let id = layerfs_home::home::opened(home)?.id;
    let mut spec = record::spec(home, &id, record)?;
    let bundle = stage(events, "bundle-validation", super::bundle::bundle)?;
    // the docker worker's container runs the bundle's image when the
    // machine names none (P5)
    if spec.kind == Platform::Docker && spec.os_image.is_none() {
        spec.os_image = Some(stage(events, "sandbox-image", || {
            super::image::docker_default(home)
        })?);
    }
    let Some(running) = stage(events, "provider-and-worker", || {
        adapters::start(&spec, &bundle).map_err(refusal)
    })?
    else {
        return Ok(None);
    };
    let endpoint = endpoint_of(&running.endpoint);
    // a worker of another build just started means the bundle is older than
    // this build: a stop and start would start it again (it stays recorded)
    let checked = stage(events, "worker-compatibility", || {
        let hello = crate::worker::hello(&endpoint)?;
        let address = endpoint.address.to_string();
        if let Some(stale) = hello.stale_bundle(name, &bundle.dir, &address) {
            return Err(stale);
        }
        Ok(())
    });
    {
        let _lock = layerfs_home::home::lock::take(home)?;
        match &checked {
            Ok(()) => save_running(home, name, &running)?,
            Err(error) => record::save_unready(home, name, &running, &error.message)?,
        }
    }
    checked?;
    Ok(Some(endpoint))
}

/// Machine `name`'s `start.lock`, held until dropped. Starting, creating and
/// removing a machine take it, always before the home lock, so a start
/// never runs beside a create or a remove of the same VM.
/// A wait for it, at most [`START_WAIT`], is not a retry.
pub type StartLock = layerfs_home::home::entity_lock::EntityLock;

pub fn start_lock(home: &Path, name: &str) -> Result<StartLock, SdkRefusal> {
    let path = layerfs_home::home::paths::machine_dir(home, name).join("start.lock");
    let held = format!("machine {name} was being started, made or removed by another process");
    layerfs_home::home::entity_lock::take(&path, START_WAIT, &held)
}

fn endpoint_of(endpoint: &adapters::Endpoint) -> Endpoint {
    Endpoint {
        address: endpoint.address,
        secret: endpoint.secret.clone(),
    }
}

/// The recorded worker, when it answers its secret and matches this build.
/// Another build is refused, never replaced by a start over it.
fn answering(home: &Path, name: &str) -> Result<Option<Endpoint>, SdkRefusal> {
    let Some(recorded) = record::worker(home, name)? else {
        return Ok(None);
    };
    let endpoint = endpoint_of(&recorded);
    match crate::worker::hello(&endpoint) {
        Ok(hello) => match hello.mismatch(&endpoint.address.to_string()) {
            Some(refusal) => Err(refusal),
            None => Ok(Some(endpoint)),
        },
        Err(_) => Ok(None),
    }
}
