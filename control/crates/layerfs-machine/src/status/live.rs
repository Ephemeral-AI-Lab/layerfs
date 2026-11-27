//! A live read (`machine get --with live`, `machine list --with live`;
//! call surface 2.0, review fix 1): one probe of each machine's provider
//! and one hello to its worker, all machines at the same time, within
//! [`PROBE`], starting nothing.
//!
//! What a probe finds is the machine's state: `ready` (its worker answers
//! and is this build's), `stopped`, or `broken` with `engine_down` (Docker
//! is not running), `not_answering` (it runs and its worker does not
//! answer), `outdated` (its worker is of another build), `missing`,
//! `failed` or `unsupported`. A probe that fails or does not answer in
//! time is `unknown`, which is never recorded; a command it still runs
//! then is killed with its process group and reaped by the probe's thread.
//!
//! The record is written only when the state word or its reason changed,
//! and only that write moves the home's generation; when the probe
//! answered is kept beside the record (`observed.json`), which the
//! generation does not cover. An uncertain start or stop whose owner is
//! gone is settled here (correction C13): once the probe answers, the `op`
//! is dropped with the write that records what was seen. An uncertain
//! create or remove is `machine remove`'s to clear, and a machine a live
//! process holds is not probed: its `op` is its state. Nor is a machine
//! whose record alone makes it broken (a removed platform's, `unsupported`;
//! an earlier build's with no `vm_name`, `outdated`): its record's state,
//! reason and fix win over the live read (UI pass 2, F1).
use super::super::record::adapter_spec::{self as record, refusal};
use super::super::record::machine::{MachineRecord, CREATE, REMOVE};
use crate::platform::{self as adapters, Code, Spec};
use crate::route::{self, Endpoint, Hello};
use crate::Related;
use layerfs_home::record::intent::Standing;
use layerfs_home::reply::state::{Reason, State};
use layerfs_home::{
    disk::{self, paths},
    lock::home as lock,
    record::observed,
};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// How long a live read waits for its probes, all together. A probe that
/// has not answered by then is `unknown`.
const PROBE: Duration = Duration::from_secs(10);
/// How long a probe waits for a worker's hello: a forward that accepts
/// with no worker behind it must not use up the probe's time.
const HELLO: Duration = Duration::from_secs(2);

/// What a live read saw of one machine.
#[derive(Clone, Debug)]
pub(super) struct Seen {
    /// Its state: the probe's, or `unknown`.
    pub state: State,
    /// The sentence behind a broken or unknown state.
    pub message: Option<String>,
    /// When: now for a probe that answered, else when LayerFS last knew.
    pub at: Option<i64>,
    /// Free bytes of its data disk.
    disk_free: Option<u64>,
    /// Its provider tool's version.
    tool_version: Option<String>,
    /// What its worker said of itself.
    worker: Option<Hello>,
}

impl Seen {
    /// The `live` section of `machine.get`: `{ disk_free_bytes,
    /// tool_version, worker: { protocol, bundle } }`, null what was not read.
    pub(super) fn section(&self) -> SdkValue {
        let free = self.disk_free.and_then(|bytes| i64::try_from(bytes).ok());
        SdkValue::map([
            (
                "disk_free_bytes",
                free.map_or(SdkValue::Null, SdkValue::Int),
            ),
            (
                "tool_version",
                SdkValue::opt_text(self.tool_version.clone()),
            ),
            (
                "worker",
                self.worker.as_ref().map_or(SdkValue::Null, Hello::to_sdk),
            ),
        ])
    }
}

/// The `live` section of a machine that is not probed. One whose platform
/// has no provider (`native`) runs on the host's own OS: its data disk is
/// the one that holds the home, read with one `statvfs`; it has no
/// provider tool, and its worker, if any, is not asked, so those are null.
/// One an `op` holds, one of a removed platform or one with no `vm_name`:
/// nothing was read.
pub(super) fn unprobed(home: &Path, record: &MachineRecord) -> SdkValue {
    let own_os = record.removed().is_none() && record.platform().is_ok_and(|p| !p.provided());
    let free = own_os
        .then(|| disk::filesystem_available(home).ok())
        .flatten()
        .and_then(|bytes| i64::try_from(bytes).ok());
    SdkValue::map([
        (
            "disk_free_bytes",
            free.map_or(SdkValue::Null, SdkValue::Int),
        ),
        ("tool_version", SdkValue::Null),
        ("worker", SdkValue::Null),
    ])
}

/// What one probe found, before it is compared with the record.
struct Found {
    /// The state word and reason: `None` is ready, `Some(None)` stopped.
    broken: Option<Option<Reason>>,
    message: Option<String>,
    disk_free: Option<u64>,
    tool_version: Option<String>,
    worker: Option<Hello>,
}

impl Found {
    fn of(broken: Option<Option<Reason>>, message: Option<String>) -> Self {
        Found {
            broken,
            message,
            disk_free: None,
            tool_version: None,
            worker: None,
        }
    }
}

/// One probe: the provider's answer, then one hello when it says the
/// machine runs. `Err` is a probe that could not be made.
fn probe(
    spec: &Spec,
    endpoint: Option<&Endpoint>,
    stop: Instant,
) -> std::result::Result<Found, String> {
    // the provider is asked of the VM alone: the one hello is ours, which
    // says what build its worker is
    let spec = Spec {
        worker: None,
        ..spec.clone()
    };
    let status = match adapters::status_within(&spec, stop) {
        Ok(status) => status,
        // this host cannot run it at all: its tool is not installed
        Err(error) if error.code == Code::Unsupported => {
            let message = refusal(error).message;
            return Ok(Found::of(Some(Some(Reason::Unsupported)), Some(message)));
        }
        Err(error) => return Err(refusal(error).message),
    };
    let broken = |reason, message: &str| Found {
        disk_free: status.free_disk,
        tool_version: status.tool_version.clone(),
        ..Found::of(Some(Some(reason)), Some(message.to_owned()))
    };
    Ok(match status.kernel {
        None => {
            let why = status.down_reason.clone();
            let why = why.unwrap_or_else(|| "its container engine does not answer".to_owned());
            broken(Reason::EngineDown, &why)
        }
        Some(adapters::State::Missing) => broken(
            Reason::Missing,
            &format!("its provider holds no VM named {}", spec.vm_name),
        ),
        Some(adapters::State::Failed) => broken(Reason::Failed, "its provider reports it broken"),
        Some(adapters::State::Stopped) => Found {
            tool_version: status.tool_version.clone(),
            ..Found::of(Some(None), None)
        },
        Some(adapters::State::Running) => {
            let Some(endpoint) = endpoint else {
                return Ok(broken(
                    Reason::NotAnswering,
                    "it runs, and no worker is recorded for it",
                ));
            };
            match route::hello_within(endpoint, HELLO) {
                Err(unanswered) => broken(Reason::NotAnswering, &unanswered.message),
                Ok(hello) => {
                    let mismatch = hello.mismatch(&endpoint.address.to_string());
                    let mut found = match mismatch {
                        Some(refusal) => broken(Reason::Outdated, &refusal.message),
                        None => Found {
                            disk_free: status.free_disk,
                            tool_version: status.tool_version.clone(),
                            ..Found::of(None, None)
                        },
                    };
                    found.worker = Some(hello);
                    found
                }
            }
        }
    })
}

/// Whether a live read probes the machine `record` describes: not one
/// whose platform has no provider (`native`), not one of a removed platform, not one whose record has no
/// `vm_name` (an earlier build's, `broken` `outdated` by its record alone),
/// not one a live process holds, and not one a create or remove left
/// unresolved. A machine not probed keeps the state its record gives it,
/// with its reason and fix: a live read never answers it `unknown`.
fn probed(record: &MachineRecord) -> Result<bool> {
    if record.removed().is_some() || !record.platform()?.provided() {
        return Ok(false);
    }
    if record.nameless() {
        return Ok(false);
    }
    Ok(match record.op() {
        Some(op) => match op.standing() {
            Standing::Running => false,
            Standing::Unresolved => !matches!(op.name(), CREATE | REMOVE),
            Standing::Stale => true,
        },
        None => true,
    })
}

/// `record` as it is once the probe's finding is recorded: `running` or
/// `stopped` with no reason, or broken for the reason found. An `op` no
/// live process owns goes with it (a certain one meant nothing; an
/// uncertain start or stop is settled by what was seen).
fn applied(record: MachineRecord, found: &Found) -> MachineRecord {
    let record = record.without_op();
    match found.broken {
        None => record.with("state", SdkValue::text("running")).mended(),
        Some(None) => record.with("state", SdkValue::text("stopped")).mended(),
        Some(Some(reason)) => {
            record.broken_as(reason, found.message.as_deref().unwrap_or_default())
        }
    }
}

/// The state word and reason of `state`.
fn word(state: &State) -> (&'static str, Option<Reason>) {
    match state {
        State::Broken { reason, .. } => (state.word(), Some(*reason)),
        _ => (state.word(), None),
    }
}

/// Records what the probe of machine `name` found, when it differs from
/// what its record says: the write is made under the home lock, on the
/// record as it is then, and never over a call that took the machine
/// meanwhile. Answers the record the machine's state is read from.
fn note(home: &Path, name: &str, found: &Found, sandboxes: usize) -> Result<MachineRecord> {
    let read = || MachineRecord::of(home, name);
    let changes = |record: &MachineRecord| {
        let now = applied(record.clone(), found);
        (word(&record.recorded(sandboxes)) != word(&now.recorded(sandboxes))).then_some(now)
    };
    let record = read()?;
    if changes(&record).is_none() {
        return Ok(record);
    }
    let _lock = lock::take(home)?;
    let record = read()?;
    if !probed(&record)? {
        return Ok(record);
    }
    match changes(&record) {
        Some(now) => {
            now.write(home)?;
            Ok(now)
        }
        None => Ok(record),
    }
}

/// One live read of `records`: each machine that is probed
/// ([`probed`]) in parallel, outside the home lock, all within [`PROBE`].
/// `None` keeps a machine's recorded state.
pub(super) fn observe(
    home: &Path,
    records: &[MachineRecord],
    related: Related,
) -> Result<Vec<Option<Seen>>> {
    let (send, answers) = mpsc::channel();
    let late = format!("its probe did not answer within {} s", PROBE.as_secs());
    let mut found: Vec<Option<std::result::Result<Found, String>>> = Vec::new();
    let mut waiting = 0;
    let until = Instant::now() + PROBE;
    // the probes' commands are stopped just after the read stops waiting,
    // so a late probe is reported as late, not as its stopped command
    let stop = until + Duration::from_millis(200);
    for (index, machine) in records.iter().enumerate() {
        if !probed(machine)? {
            found.push(None);
            continue;
        }
        let spec = record::provider_spec(home, machine);
        let endpoint = record::worker(home, machine.name());
        match (spec, endpoint) {
            (Ok(spec), Ok(endpoint)) => {
                let send = send.clone();
                let endpoint = endpoint.map(|endpoint| Endpoint {
                    address: endpoint.address,
                    secret: endpoint.secret,
                });
                std::thread::spawn(move || {
                    drop(send.send((index, probe(&spec, endpoint.as_ref(), stop))));
                });
                found.push(Some(Err(late.clone())));
                waiting += 1;
            }
            (Err(why), _) | (_, Err(why)) => found.push(Some(Err(why.message))),
        }
    }
    while waiting > 0 {
        let Some(left) = until.checked_duration_since(Instant::now()) else {
            break;
        };
        let Ok((index, answer)) = answers.recv_timeout(left) else {
            break;
        };
        waiting -= 1;
        found[index] = Some(answer);
    }
    let mut seen = Vec::new();
    for (machine, found) in records.iter().zip(found) {
        let name = machine.name();
        let file = paths::machine_record(home, name);
        seen.push(match found {
            None => None,
            Some(Err(why)) => Some(Seen {
                state: State::Unknown {
                    observed_ms: observed::last_known(&file),
                },
                message: Some(why),
                at: None,
                disk_free: None,
                tool_version: None,
                worker: None,
            }),
            Some(Ok(found)) => {
                let sandboxes = (related.sandboxes_on)(home, name)?.len();
                let now = note(home, name, &found, sandboxes)?;
                let at = disk::now_ms();
                // when it was seen is not a record: no generation moves
                let _ = observed::write(&file, at);
                let state = now.recorded(sandboxes);
                Some(Seen {
                    message: now.message(&state, sandboxes),
                    state,
                    at: Some(at),
                    disk_free: found.disk_free,
                    tool_version: found.tool_version,
                    worker: found.worker,
                })
            }
        });
    }
    Ok(seen)
}
