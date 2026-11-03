//! Where a call runs (#535 §6.4, §7.1, #536 §9.2): [`plan`] decides it once,
//! from the parsed Store and the machine, and the [`Executor`] it answers
//! runs it there. A PostgreSQL Store's calls run in this process; a sqlite
//! Store's run in this process on a machine whose platform runs them
//! locally (`native`, except on Windows, whose Stores are in its worker),
//! else in that machine's worker, `docker` on every host included (G8).
//! The third executor, a Sandbox's daemon reached directly (G4), arrives
//! with direct Workspace calls (#535 phase 6).
use crate::common::home::{paths, read_record};
use crate::common::outcome::{refused, Outcome};
use crate::machine::worker::{self, Endpoint, Unreached};
use crate::store::record::StoreRef;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Who runs a call.
pub(crate) enum Executor {
    /// This process.
    Local,
    /// A machine's worker.
    Worker(Worker),
}

/// Whether a call may start its machine (#535 P4, #536 §10b).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Start {
    /// The call needs the worker: a stopped machine is started first.
    OnDemand,
    /// A list, get or status call: it uses the worker the records name, or
    /// is refused `Unavailable`, and never starts the machine.
    Never,
}

/// One machine's worker. [`plan`] reads no worker record and dials
/// nothing: the first call dials the worker its records name, and later
/// calls of the same operation (a move's steps) use the worker it reached.
pub(crate) struct Worker {
    home: PathBuf,
    machine: String,
    start: Start,
    endpoint: Option<Endpoint>,
}

/// Where a call on Store `store` runs.
pub(crate) fn plan(home: &Path, store: &StoreRef, start: Start) -> Result<Executor> {
    match store {
        StoreRef::Postgres { .. } => Ok(Executor::Local),
        StoreRef::Sqlite { .. } => on_machine(home, store.machine()?, start),
    }
}

/// Where a call on machine `machine` runs: its Sandboxes' host operations,
/// and the Store calls of the sqlite Stores on it.
pub(crate) fn on_machine(home: &Path, machine: &str, start: Start) -> Result<Executor> {
    if local(home, machine, start)? {
        return Ok(Executor::Local);
    }
    Ok(Executor::Worker(Worker {
        home: home.to_owned(),
        machine: machine.to_owned(),
        start,
        endpoint: None,
    }))
}

impl Worker {
    /// The machine whose worker this is.
    pub(crate) fn machine(&self) -> &str {
        &self.machine
    }

    /// Makes sure the worker is up before an operation of several steps
    /// marks anything (`store.remove`, `store.move`): a stopped machine is
    /// started now, before the home lock, not by its first step. This is a
    /// readiness wait, not a call.
    pub(crate) fn ready(&mut self) -> Result<()> {
        if self.endpoint.is_none() {
            self.endpoint = Some(match self.start {
                Start::OnDemand => started(&self.home, &self.machine)?,
                Start::Never => recorded(&self.home, &self.machine)?,
            });
        }
        Ok(())
    }

    /// Runs worker operation `op` once with `request`; its events go to
    /// `events`. No ping comes first (#535 R1): the call dials the worker
    /// once. Only when nothing was sent ([`Unreached`]: the connection, the
    /// secret or its answer failed) on this operation's first call does a
    /// [`Start::OnDemand`] caller start the machine and send the call once,
    /// to the worker that start recorded. That is not a retry: the first
    /// attempt never reached a worker. A failure after the request went out
    /// is the call's outcome (`Unknown` when its answer was lost), never
    /// resent.
    pub(crate) fn call(
        &mut self,
        op: &str,
        request: SdkValue,
        events: &mut dyn FnMut(SdkValue),
    ) -> Outcome {
        let request = worker::request(op, request)?;
        let send = |endpoint: &Endpoint, events: &mut dyn FnMut(SdkValue)| {
            worker::send(endpoint, op, &request, events, None)
        };
        if let Some(endpoint) = &self.endpoint {
            return send(endpoint, events).unwrap_or_else(|Unreached(refusal)| Err(refusal));
        }
        let tried = recorded(&self.home, &self.machine);
        let not_sent = match &tried {
            Ok(endpoint) => match send(endpoint, events) {
                Ok(outcome) => {
                    self.endpoint = tried.ok();
                    return outcome;
                }
                Err(Unreached(refusal)) => refusal,
            },
            Err(refusal) => refusal.clone(),
        };
        if self.start == Start::Never {
            return Err(not_sent);
        }
        let endpoint = started_after(&self.home, &self.machine, tried.ok().as_ref())?;
        let endpoint = self.endpoint.insert(endpoint);
        send(endpoint, events).unwrap_or_else(|Unreached(refusal)| Err(refusal))
    }
}

/// Whether machine `machine`'s calls run in this process: its platform's
/// executor on this host (G8). A machine being removed is refused `InUse`
/// to a call that may start it.
#[cfg(feature = "machine")]
fn local(home: &Path, machine: &str, start: Start) -> Result<bool> {
    use layerfs_machine::{ExecutorKind, HostOs, Platform};
    let platform = match machine {
        "native" => Platform::Native,
        _ => {
            let record = match start {
                Start::OnDemand => crate::machine::machine_of(home, machine)?,
                Start::Never => read_record(&paths::machine_record(home, machine))?
                    .ok_or_else(|| refused("NotFound", format!("no machine named {machine}")))?,
            };
            crate::machine::platform_of(&record)?
        }
    };
    Ok(platform.executor(HostOs::current()) == ExecutorKind::Local)
}

/// Without the machine adapters (the guest worker's build): `native` is
/// this process; any other machine's worker is the one its `worker.json`
/// records, never started.
#[cfg(not(feature = "machine"))]
fn local(_: &Path, machine: &str, _: Start) -> Result<bool> {
    Ok(machine == "native")
}

/// The worker of `machine`, started first if it does not answer.
#[cfg(feature = "machine")]
fn started(home: &Path, machine: &str) -> Result<Endpoint> {
    crate::machine::running(home, machine)?.ok_or_else(|| {
        refused(
            "Failed",
            format!("machine {machine} has no worker to start"),
        )
    })
}

#[cfg(not(feature = "machine"))]
fn started(home: &Path, machine: &str) -> Result<Endpoint> {
    recorded(home, machine)
}

/// The worker of `machine` once started, after a call to `tried` (its
/// recorded worker, if any) sent nothing.
#[cfg(feature = "machine")]
fn started_after(home: &Path, machine: &str, tried: Option<&Endpoint>) -> Result<Endpoint> {
    crate::machine::start_after(home, machine, tried)
}

/// Without the machine adapters nothing is started: the call that sent
/// nothing is refused.
#[cfg(not(feature = "machine"))]
fn started_after(_: &Path, machine: &str, _: Option<&Endpoint>) -> Result<Endpoint> {
    Err(refused(
        "Unavailable",
        format!("machine {machine} is not running"),
    ))
}

/// The worker `machine`'s `worker.json` records, starting nothing; refused
/// `Unavailable` when it records none. A recorded worker that is gone
/// fails when called.
fn recorded(home: &Path, machine: &str) -> Result<Endpoint> {
    let Some(value) = read_record(&paths::worker_record(home, machine))? else {
        return Err(refused(
            "Unavailable",
            format!("machine {machine} is not running"),
        ));
    };
    let address = value
        .get_text("address")
        .and_then(|a| a.parse().ok())
        .ok_or_else(|| {
            refused(
                "Failed",
                format!("machine {machine}'s worker.json has no address"),
            )
        })?;
    Ok(Endpoint {
        address,
        secret: value.get_text("secret").unwrap_or("").to_owned(),
    })
}
