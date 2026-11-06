//! Where a call runs (#535 §6.4, §7.1, #536 §9.2): [`plan`] decides it once,
//! from the parsed Store and the machine, and the [`Executor`] it answers
//! runs it there. A PostgreSQL Store's calls run in this process; a sqlite
//! Store's run in this process on a machine whose platform runs them
//! locally (`native`, except on Windows, whose Stores are in its worker),
//! else in that machine's worker, `docker` on every host included (G8).
//! A Sandbox's daemon reached directly (G4) is not an executor here: its
//! Workspace calls take that route in `sandbox/direct.rs`.
use crate::record::StoreRef;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_machine::worker::{self, Endpoint, Unreached};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Who runs a call.
pub enum Executor {
    /// This process.
    Local,
    /// A machine's worker.
    Worker(Worker),
}

/// One machine's worker. [`plan`] reads no worker record and dials
/// nothing: the first call dials the worker its records name, and later
/// calls of the same operation (a move's steps) use the worker it reached.
/// Nothing here starts a machine (#535 D11): a call whose worker is not
/// recorded or does not answer is refused `Unavailable`, naming
/// `layerfs machine start`.
pub struct Worker {
    home: PathBuf,
    machine: String,
    endpoint: Option<Endpoint>,
}

/// Where a call on Store `store` runs.
pub fn plan(home: &Path, store: &StoreRef) -> Result<Executor> {
    match store {
        StoreRef::Postgres { .. } => Ok(Executor::Local),
        StoreRef::Sqlite { .. } => on_machine(home, store.machine()?),
    }
}

/// Where a call on machine `machine` runs: its Sandboxes' host operations,
/// and the Store calls of the sqlite Stores on it.
pub fn on_machine(home: &Path, machine: &str) -> Result<Executor> {
    if layerfs_machine::runs_here(home, machine)? {
        return Ok(Executor::Local);
    }
    Ok(Executor::Worker(Worker {
        home: home.to_owned(),
        machine: machine.to_owned(),
        endpoint: None,
    }))
}

impl Worker {
    /// The worker an operation of several steps uses (`store.remove`,
    /// `store.push`, `store.pull`), read before it marks anything: a machine with no
    /// recorded worker is refused now, naming `layerfs machine start`.
    pub fn ready(&mut self) -> Result<()> {
        if self.endpoint.is_none() {
            self.endpoint = Some(recorded(&self.home, &self.machine)?);
        }
        Ok(())
    }

    /// Runs worker operation `op` once with `request`; its events go to
    /// `events`. No ping comes first (#535 R1): the call dials the worker
    /// its records name, once. A failure after the request went out is the
    /// call's outcome (`Unknown` when its answer was lost), never resent.
    pub fn call(
        &mut self,
        op: &str,
        request: SdkValue,
        events: &mut dyn FnMut(SdkValue),
    ) -> Outcome {
        self.attempt(op, request, events)
            .unwrap_or_else(|Unreached(refusal)| Err(refusal))
    }

    /// [`Worker::call`], answering [`Unreached`] when the request was not
    /// sent (no worker recorded, or it did not answer its secret): the
    /// machine is stopped or its worker gone, which a list, get or status
    /// call may report as down (P4). The refusal names `layerfs machine
    /// start`; nothing is started (#535 D11).
    pub fn attempt(
        &mut self,
        op: &str,
        request: SdkValue,
        events: &mut dyn FnMut(SdkValue),
    ) -> std::result::Result<Outcome, Unreached> {
        let request = match worker::request(op, request) {
            Ok(request) => request,
            Err(refusal) => return Ok(Err(refusal)),
        };
        if let Some(endpoint) = &self.endpoint {
            return worker::send(endpoint, op, &request, events, None)
                .map_err(|Unreached(why)| Unreached(stopped(&self.machine, why)));
        }
        let endpoint = recorded(&self.home, &self.machine).map_err(Unreached)?;
        match worker::send(&endpoint, op, &request, events, None) {
            Ok(outcome) => {
                self.endpoint = Some(endpoint);
                Ok(outcome)
            }
            Err(Unreached(why)) => Err(Unreached(stopped(&self.machine, why))),
        }
    }
}

/// The refusal of a call whose machine's worker did not answer: the
/// machine is stopped, or its worker ended. A worker of another build is
/// replaced by stopping and starting its machine. Nothing was sent.
fn stopped(machine: &str, why: SdkRefusal) -> SdkRefusal {
    if worker::is_mismatch(&why) {
        let message = format!(
            "machine {machine}: {}; replace its worker with this build's: {}",
            why.message,
            worker::replace_with(machine)
        );
        return SdkRefusal { message, ..why };
    }
    refused(
        "Unavailable",
        format!(
            "machine {machine}'s worker did not answer ({}); start it with `layerfs machine start {machine}`",
            why.message
        ),
    )
}

/// The worker `machine`'s `worker.json` records, starting nothing; refused
/// `Unavailable` when it records none. A recorded worker that is gone
/// fails when called.
fn recorded(home: &Path, machine: &str) -> Result<Endpoint> {
    let Some(worker) = layerfs_home::secrets::worker_secret(home, machine)? else {
        return Err(refused(
            "Unavailable",
            format!(
                "machine {machine} is not running; start it with `layerfs machine start {machine}`"
            ),
        ));
    };
    Ok(Endpoint {
        address: worker.address,
        secret: worker.secret,
    })
}
