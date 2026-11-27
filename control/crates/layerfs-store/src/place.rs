//! Where a call runs (#535 §6.4, §7.1, #536 §9.2): [`plan`] decides it once,
//! from the parsed Store and the machine, and the [`Executor`] it answers
//! runs it there. A PostgreSQL Store's calls run in this process; a sqlite
//! Store's run in this process on a machine whose platform runs them
//! locally (`native`, except on Windows, whose Stores are in its worker),
//! else in that machine's worker, `docker` on every host included (G8).
//! A Sandbox's daemon reached directly (G4) is not an executor here: its
//! Workspace calls take that route in `sandbox/direct.rs`.
use crate::record::StoreRef;
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::{refused, with_fix, with_next, Outcome, FIX};
use layerfs_machine::worker::{self, Budget, Endpoint, EventSink, Unreached};
use std::path::{Path, PathBuf};
use std::time::Duration;

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
/// `layerfs machine start`. A clone dials the same worker:
/// `project.init` sends its parts on several connections at once.
#[derive(Clone)]
pub struct Worker {
    home: PathBuf,
    machine: String,
    endpoint: Option<Endpoint>,
}

/// Where a call on Store `store` runs.
pub fn plan(home: &Path, store: &StoreRef) -> Result<Executor> {
    match store {
        StoreRef::Postgres { .. } => Ok(Executor::Local),
        StoreRef::Sqlite { name, machine } => {
            on_machine(home, machine).map_err(|refusal| blocked(name, machine, refusal))
        }
    }
}

/// A Store call's refusal for its machine whose record has no `vm_name`
/// (LF10) or whose VM a live read found gone (area 10): the Store reads
/// `blocked` by that machine, `outdated` or `missing` (`store list`), so
/// the call is `Unavailable`, as for the machine's other not-ready states,
/// with that `blocked_by` beside it and the machine's own fix, which the
/// call boundary widens to the chain of what is on it (UI2-8). Any other
/// refusal is kept.
pub(crate) fn blocked(store: &str, machine: &str, refusal: SdkRefusal) -> SdkRefusal {
    use layerfs_home::state::{BlockedBy, Reason};
    use layerfs_machine::record::{no_vm_name_fix, Holders};
    let bare = no_vm_name_fix(machine, Holders::default());
    if refusal.code != "Failed" || refusal.detail.get_text(FIX) != Some(bare.as_str()) {
        return refusal;
    }
    // `vm_present`'s refusal says the VM is gone; `provider_name`'s that
    // the record has no `vm_name`
    let reason = match refusal.message.contains("'s VM is gone") {
        true => Reason::Missing,
        false => Reason::Outdated,
    };
    let by = BlockedBy {
        thing: "machine",
        handle: machine.to_owned(),
        reason: reason.word(),
    };
    let message = format!(
        "Store {store} is blocked by machine {machine} ({}): {}",
        reason.word(),
        refusal.message
    );
    let code = "Unavailable".to_owned();
    with_next(
        SdkRefusal {
            code,
            message,
            ..refusal
        },
        "blocked_by",
        by.to_sdk(),
    )
}

/// Where a call on machine `machine` runs: its Sandboxes' host operations,
/// and the Store calls of the sqlite Stores on it.
pub fn on_machine(home: &Path, machine: &str) -> Result<Executor> {
    if let Some(record) = layerfs_machine::record::MachineRecord::read(home, machine)? {
        record.not_held()?;
    }
    on_machine_read(home, machine)
}

/// How long a read ([`Worker::read`]) waits for its machine's worker: the
/// connection, the answer to the secret, and the whole answer (12 s in
/// all; layerfs-project#178).
pub const READ: Budget = Budget {
    connect: Duration::from_secs(2),
    hello: Duration::from_secs(2),
    answer: Duration::from_secs(8),
};

/// [`on_machine`] for a call that only reads (layerfs-project#179 A):
/// where it runs while a machine operation holds the machine's intent too,
/// since a start that is failing is when its worker's log is wanted. A
/// worker that is stopping answers `Unavailable` when called. No lease,
/// no Store, no write.
pub fn on_machine_read(home: &Path, machine: &str) -> Result<Executor> {
    if layerfs_machine::runs_here(home, machine)? {
        return Ok(Executor::Local);
    }
    // a machine whose record has no `vm_name` has no worker this build
    // reaches: refused `Failed` before anything is dialled (V20-8), with
    // its remove as the fix, which the call boundary widens to the chain
    // of what is on it (UI2-8; RC-B F3); a Store call answers it
    // `Unavailable` with the Store's `blocked_by` ([`plan`], LF10)
    if let Some(record) = layerfs_machine::record::MachineRecord::read(home, machine)? {
        if record.nameless() {
            record.provider_name()?;
        }
        // a VM a live read found gone: likewise, nothing is dialled (area 10)
        record.vm_present()?;
    }
    Ok(Executor::Worker(Worker {
        home: home.to_owned(),
        machine: machine.to_owned(),
        endpoint: None,
    }))
}

impl Worker {
    /// The worker an operation of several steps uses (`store.remove`,
    /// `store.move`), read before it marks anything: a machine with no
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

    /// Runs read-only worker operation `op` once with `request`, waiting at
    /// most `budget` for each step. A worker that is not recorded, does
    /// not answer or does not answer in time is `Unavailable`: the
    /// operation changes nothing, so a lost answer is not `Unknown`.
    pub fn read(&mut self, op: &str, request: SdkValue, budget: Budget) -> Outcome {
        let request = worker::request(op, request)?;
        let endpoint = match &self.endpoint {
            Some(endpoint) => endpoint.clone(),
            None => recorded(&self.home, &self.machine)?,
        };
        let outcome = worker::send_within(&endpoint, op, &request, budget)
            .map_err(|Unreached(why)| stopped(&self.machine, why))?;
        self.endpoint = Some(endpoint);
        outcome.map_err(|refusal| match refusal.code.as_str() {
            "Unknown" => refused(
                "Unavailable",
                format!(
                    "machine {}'s worker did not answer {op} in time ({}); {op} changes nothing",
                    self.machine, refusal.message
                ),
            ),
            _ => refusal,
        })
    }

    /// [`Worker::call`] whose `events` may refuse one: the connection then
    /// closes, the worker stops at its next event, and the call's outcome
    /// is that refusal (`project.export`'s local writer failed).
    pub fn call_until(
        &mut self,
        op: &str,
        request: SdkValue,
        events: &mut EventSink<'_>,
    ) -> Outcome {
        let request = worker::request(op, request)?;
        self.attempt_framed(op, &request, events)
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
        let mut events = |event| {
            events(event);
            Ok(())
        };
        self.attempt_framed(op, &request, &mut events)
    }

    /// [`Worker::attempt`] with `request` already framed
    /// ([`worker::request`], on the thread whose home id and step timing it
    /// carries).
    pub fn attempt_framed(
        &mut self,
        op: &str,
        request: &[u8],
        events: &mut EventSink<'_>,
    ) -> std::result::Result<Outcome, Unreached> {
        if let Some(endpoint) = &self.endpoint {
            return worker::send(endpoint, op, request, events, None)
                .map_err(|Unreached(why)| Unreached(stopped(&self.machine, why)));
        }
        let endpoint = recorded(&self.home, &self.machine).map_err(Unreached)?;
        match worker::send(&endpoint, op, request, events, None) {
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
        let fix = format!(
            "layerfs machine stop {machine} && layerfs machine start {machine}",
            machine = shell_word(machine)
        );
        return with_fix(SdkRefusal { message, ..why }, fix);
    }
    let refusal = refused(
        "Unavailable",
        format!(
            "machine {machine}'s worker did not answer ({}); start it with `layerfs machine start {machine}`",
            why.message
        ),
    );
    with_fix(
        refusal,
        format!(
            "layerfs machine start {machine}",
            machine = shell_word(machine)
        ),
    )
}

/// The worker `machine`'s `worker.json` records, starting nothing; refused
/// `Unavailable` when it records none. A recorded worker that is gone
/// fails when called.
fn recorded(home: &Path, machine: &str) -> Result<Endpoint> {
    let Some(worker) = layerfs_home::secrets::worker_secret(home, machine)? else {
        let refusal = refused(
            "Unavailable",
            format!(
                "machine {machine} is not running; start it with `layerfs machine start {machine}`"
            ),
        );
        return Err(with_fix(
            refusal,
            format!(
                "layerfs machine start {machine}",
                machine = shell_word(machine)
            ),
        ));
    };
    Ok(Endpoint {
        address: worker.address,
        secret: worker.secret,
    })
}
