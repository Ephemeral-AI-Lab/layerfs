//! Where a call runs (#535 §6.4, §7.1, #536 §9.2): [`plan`] decides it once,
//! from the parsed Store and the machine, and the [`Executor`] it answers
//! runs it there. A PostgreSQL Store's calls run in this process; a sqlite
//! Store's run in this process on a machine whose platform runs them
//! locally (`native`, except on Windows, whose Stores are in its worker),
//! else in that machine's worker, `docker` on every host included (G8).
//! A Sandbox's daemon reached directly (G4) is not an executor here: its
//! Workspace calls take that route in `sandbox/direct.rs`.
//!
//! It also answers whether a sqlite Store may be placed on a machine at all
//! (#545 B0-150, B7-223): [`require_store_home`] refuses `StoragePlacement`
//! for a Store whose file the machine does not keep, and both placement
//! paths — `sandbox.create` and `store.push` — ask it before they reach
//! anything. The machine record's `direct_store_access`, the typed backend
//! capability's one owner of the fact, is the only input: an absent or
//! unknown capability fails closed. A PostgreSQL Store is on no machine and
//! is never refused.
use crate::common::outcome::{refused, Outcome};
use crate::machine::record::MachineRecord;
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

/// One machine's worker. [`plan`] reads no worker record and dials
/// nothing: the first call dials the worker its records name, and later
/// calls of the same operation (a move's steps) use the worker it reached.
/// Nothing here starts a machine (#535 D11): a call whose worker is not
/// recorded or does not answer is refused `Unavailable`, naming
/// `layerfs machine start`.
pub(crate) struct Worker {
    home: PathBuf,
    machine: String,
    endpoint: Option<Endpoint>,
}

/// Where a call on Store `store` runs.
pub(crate) fn plan(home: &Path, store: &StoreRef) -> Result<Executor> {
    match store {
        StoreRef::Postgres { .. } => Ok(Executor::Local),
        StoreRef::Sqlite { .. } => on_machine(home, store.machine()?),
    }
}

/// Where a call on machine `machine` runs: its Sandboxes' host operations,
/// and the Store calls of the sqlite Stores on it.
pub(crate) fn on_machine(home: &Path, machine: &str) -> Result<Executor> {
    if local(home, machine)? {
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
    pub(crate) fn ready(&mut self) -> Result<()> {
        if self.endpoint.is_none() {
            self.endpoint = Some(recorded(&self.home, &self.machine)?);
        }
        Ok(())
    }

    /// Runs worker operation `op` once with `request`; its events go to
    /// `events`. No ping comes first (#535 R1): the call dials the worker
    /// its records name, once. A failure after the request went out is the
    /// call's outcome (`Unknown` when its answer was lost), never resent.
    pub(crate) fn call(
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
    pub(crate) fn attempt(
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

/// Whether machine `machine`'s calls run in this process: its platform's
/// executor on this host (G8).
#[cfg(feature = "machine")]
fn local(home: &Path, machine: &str) -> Result<bool> {
    use layerfs_machine::{ExecutorKind, HostOs, Platform};
    let platform = match machine {
        "native" => Platform::Native,
        _ => crate::machine::record::MachineRecord::of(home, machine)?.platform()?,
    };
    Ok(platform.executor(HostOs::current()) == ExecutorKind::Local)
}

/// Without the machine adapters (the guest worker's build): `native` is
/// this process; any other machine's worker is the one its `worker.json`
/// records.
#[cfg(not(feature = "machine"))]
fn local(_: &Path, machine: &str) -> Result<bool> {
    Ok(machine == "native")
}

/// The worker `machine`'s `worker.json` records, starting nothing; refused
/// `Unavailable` when it records none. A recorded worker that is gone
/// fails when called.
fn recorded(home: &Path, machine: &str) -> Result<Endpoint> {
    let Some(worker) = crate::common::secrets::worker_secret(home, machine)? else {
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

/// Where a sqlite Store's file lives on a machine (#545 B0-150, B7-223):
/// on the machine, whose worker or the caller's process opens it there —
/// Family 1 — or on no disk of ours, a machine that stops with its
/// instances and keeps no Store between them — Family 2, ephemeral remote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StoreHome {
    /// The machine keeps the Store's file; its calls open it there.
    Machine,
    /// The machine keeps no Store of ours: no worker, no local file.
    Remote,
    /// The record declares no `direct_store_access`: an old record, or a
    /// backend this build does not know. Never guessed from a platform.
    Unknown,
}

/// The machine record field the typed backend capability owns (#551
/// B0-150): whether the machine's backend reaches a sqlite Store's file
/// directly. It is the one owner of "a Store can live here": the record
/// carries it from the backend's `ProviderCapabilities`, `machine.get`
/// exposes it, and this policy reads it. The record is the only fact, so the
/// same record answers the same on every host.
const DIRECT_STORE_ACCESS: &str = "direct_store_access";

/// A machine's store home, the backend a refusal names, and why the machine
/// keeps no Store when it does not.
pub(crate) struct MachinePlacement {
    pub(crate) home: StoreHome,
    /// The backend the machine's record names (`docker`, `native`,
    /// `namespaces`); `Null` when the record names none, or the machine has
    /// no record. A record fact, so it is the same on every host.
    pub(crate) backend: SdkValue,
    /// Why the machine keeps no Store of ours, or why the record does not
    /// say; `None` when it keeps one.
    pub(crate) reason: Option<String>,
}

/// Machine `machine`'s store home, from the machine record alone: the
/// backend's typed `direct_store_access`, written into the record when the
/// machine is made (#551 B0-150), says whether *this* machine keeps a sqlite
/// Store's file. Present and `true` is [`StoreHome::Machine`]; present and
/// `false` is [`StoreHome::Remote`]; present and anything else is refused
/// `InvalidInput`. A record written before the field reads through its
/// platform's declared backend, the same typed capability and the same
/// answer on every host. A platform that names no backend this build knows
/// has no fallback: an ephemeral/remote backend that declares no capability
/// is [`StoreHome::Unknown`] and placement refuses it, never defaulting to
/// "the machine keeps its Store". A machine with no record at all is
/// `Machine`: the step that resolves the machine refuses `NotFound`, and the
/// guest worker's build writes no built-in records at all.
///
/// The record is the only fact, and the decision never asks the platform
/// support table or the host it runs on: the same record must get the same
/// answer on every host, and an unknown capability fails closed.
pub(crate) fn placement(home: &Path, machine: &str) -> Result<MachinePlacement> {
    let Some(record) = MachineRecord::read(home, machine)? else {
        // a machine with no record keeps its Store as far as this guard can
        // tell: the step that resolves the machine refuses `NotFound`, and
        // the guest worker's build writes no built-in records at all
        return Ok(MachinePlacement {
            home: StoreHome::Machine,
            backend: SdkValue::Null,
            reason: None,
        });
    };
    let backend = match record.backend_name() {
        Some(backend) => SdkValue::text(backend),
        None => SdkValue::Null,
    };
    let declared = record.field(DIRECT_STORE_ACCESS).is_some();
    let (store_home, reason) = match record.direct_store_access() {
        Some(true) => (StoreHome::Machine, None),
        Some(false) => (
            StoreHome::Remote,
            Some(if declared {
                format!(
                    "its record says {DIRECT_STORE_ACCESS} is false: the backend keeps no disk \
                     of ours, so a Store there would stop with its instances"
                )
            } else {
                format!(
                    "its platform's declared backend keeps no disk of ours, so a Store there \
                     would stop with its instances"
                )
            }),
        ),
        // a value that is neither true nor false is malformed, not "does
        // not say"
        None if declared => {
            return Err(refused(
                "InvalidInput",
                format!("machine {machine}'s record `{DIRECT_STORE_ACCESS}` is not true or false"),
            ))
        }
        // the capability is unknown: this is a record whose platform names
        // no backend this build knows — an ephemeral/remote backend that
        // did not declare the capability. Fail closed rather than assuming
        // the machine keeps its Store.
        None => (
            StoreHome::Unknown,
            Some(format!(
                "its record declares no {DIRECT_STORE_ACCESS}, and its platform names no backend \
                 this build knows: the backend capability is the only fact, so the placement is \
                 refused rather than assuming the machine keeps it"
            )),
        ),
    };
    Ok(MachinePlacement {
        home: store_home,
        backend,
        reason,
    })
}

/// Refuses `StoragePlacement` unless a sqlite Store may be placed on machine
/// `machine` (#545 B0-150, B7-223): its file must stay on a machine whose
/// backend reaches it directly — Family 1 — and a Family 2 machine
/// (ephemeral, no worker, no disk of ours), or one whose record does not
/// declare the capability, is refused instead of silently routing the Store
/// there. A PostgreSQL Store is on no machine (G3) and is never refused.
/// Nothing is dialled, started or written: this decides on facts, before the
/// side effect.
pub(crate) fn require_store_home(home: &Path, store: &StoreRef, machine: &str) -> Result<()> {
    let StoreRef::Sqlite { name, .. } = store else {
        return Ok(());
    };
    let MachinePlacement {
        home: store_home,
        backend,
        reason,
    } = placement(home, machine)?;
    if store_home == StoreHome::Machine {
        return Ok(());
    }
    let reason = reason.unwrap_or_else(|| "the machine keeps no Store of ours".to_owned());
    let named = match &backend {
        SdkValue::Text(backend) => format!(" (backend {backend})"),
        _ => String::new(),
    };
    let mut refusal = refused(
        "StoragePlacement",
        format!(
            "Store {name} cannot be placed on machine {machine}{named}: {reason}; keep the \
             Store's file on a machine whose backend reaches it directly, or use a PostgreSQL \
             Store (`layerfs store connect {name}`)"
        ),
    );
    refusal.detail = SdkValue::map([
        ("store", SdkValue::text(name)),
        ("machine", SdkValue::text(machine)),
        ("backend", backend),
        ("reason", SdkValue::text(&reason)),
    ]);
    Err(refusal)
}
