//! `machine.list` and the live probes and summaries it shares with `get`, `update` and the create/connect outcome.
use super::record::{MachineDerived, MachineRecord};
use super::spec::{self as record, refusal};
use super::status::home_id;
use crate::common::context::Context;
use crate::common::contract;
use crate::common::home::paths;
use crate::common::outcome::{failed, Outcome, View};
use crate::inventory::{sandboxes_on, stores_on};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_machine as adapters;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// How long `machine.list` and `machine.get` wait for their live probes, all
/// together (#551 B8-159). A probe that has not answered by then is
/// `unknown`; the read budget is [`contract::PROBE_BUDGET_MS`].
const PROBE: Duration = Duration::from_millis(contract::PROBE_BUDGET_MS);

/// A machine's live state and, when it could not be read, why.
type Live = Option<(String, Option<String>)>;

/// The live state of each VM and ssh machine, read as `machine.status`
/// reads it: in parallel, starting nothing, outside the home lock, all within
/// [`PROBE`]. A probe that fails or does not answer in time is `unknown`, with
/// the reason; a command it still runs then is killed with its process group
/// and reaped by the probe's thread. Nothing is written back. `None` keeps
/// the record's state: `native`, or a machine being created or removed.
/// The built-in `docker` is probed like a VM: its worker is started and
/// stopped by hand (#535 D11), so a fixed `ready` would be wrong.
pub(super) fn live(home: &Path, records: &[MachineRecord]) -> Result<Vec<Live>> {
    let id = home_id(home)?;
    let (send, answers) = mpsc::channel();
    let late = format!("its probe did not answer within {} s", PROBE.as_secs());
    let mut out: Vec<Live> = vec![None; records.len()];
    let mut waiting = 0;
    let until = Instant::now() + PROBE;
    // the probes' commands are stopped just after the list stops waiting,
    // so a late probe is reported as late, not as its stopped command
    let stop = until + Duration::from_millis(200);
    for (index, record) in records.iter().enumerate() {
        if let Some(reason) =
            layerfs_sandbox::platform_policy::disabled_reason(record.platform()?.name())
        {
            out[index] = Some((record.state(None)?.to_owned(), Some(reason.to_owned())));
            continue;
        }
        if record.platform()? == adapters::Platform::Native || record.in_flight() {
            continue;
        }
        match record::spec(home, &id, record) {
            Ok(spec) => {
                let send = send.clone();
                std::thread::spawn(move || {
                    drop(send.send((index, adapters::status_within(&spec, stop))));
                });
                out[index] = Some(("unknown".to_owned(), Some(late.clone())));
                waiting += 1;
            }
            Err(why) => out[index] = Some(("unknown".to_owned(), Some(why.message))),
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
        out[index] = Some(match answer {
            Ok(status) => (status.state.as_str().to_owned(), None),
            Err(error) => ("unknown".to_owned(), Some(refusal(error).message)),
        });
    }
    Ok(out)
}

pub(super) fn summary(home: &Path, record: &MachineRecord, live: Live) -> Result<SdkValue> {
    let state = record
        .state(live.as_ref().map(|(state, _)| state.as_str()))?
        .to_owned();
    let derived = MachineDerived {
        live,
        stores: stores_on(home, record.name())?,
        sandboxes: sandboxes_on(home, record.name())?,
        worker: Some(worker(home, record.name(), &state)),
        lease: Some(lease(home, record.name())),
    };
    record.to_sdk(View::Full, &derived)
}

/// The machine's worker as its `worker.json` records it (#551 B8-156): the
/// address, when it started and its forward, and whether the machine's
/// derived state says it runs. It never dials the worker.
fn worker(home: &Path, name: &str, state: &str) -> SdkValue {
    let Ok(recorded) = crate::common::home::read_record(&paths::worker_record(home, name)) else {
        return SdkValue::map([("state", SdkValue::text("none"))]);
    };
    let Some(recorded) = recorded else {
        return SdkValue::map([("state", SdkValue::text("none"))]);
    };
    let runs = matches!(state, "ready" | "running");
    SdkValue::map([
        (
            "state",
            SdkValue::text(if runs { "running" } else { "recorded" }),
        ),
        (
            "address",
            SdkValue::opt_text(recorded.get_text("address").map(str::to_owned)),
        ),
        (
            "started",
            recorded.get("started").cloned().unwrap_or_default(),
        ),
        (
            "forward_pid",
            recorded.get("forward_pid").cloned().unwrap_or_default(),
        ),
        (
            "path",
            SdkValue::text(paths::worker_record(home, name).display().to_string()),
        ),
    ])
}

/// The machine's `start.lock` lease, probed without taking it (#551 B8-156).
fn lease(home: &Path, name: &str) -> SdkValue {
    let path = paths::machine_dir(home, name).join("start.lock");
    match crate::common::home::entity_lock::probe(&path) {
        Some(lease) => SdkValue::map([
            ("held", SdkValue::Bool(lease.held)),
            ("path", SdkValue::text(path.display().to_string())),
            ("since", lease.since.map_or(SdkValue::Null, SdkValue::Int)),
        ]),
        None => SdkValue::map([("held", SdkValue::Bool(false))]),
    }
}

/// `machine.list()`: every machine with its live state and what it
/// supports; `machine.platforms` answers what could be created here.
pub fn list(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    let home = &context.home;
    let mut records = Vec::new();
    for name in paths::names(&home.join("machine")).map_err(failed)? {
        if let Some(record) = MachineRecord::read(home, &name)? {
            records.push(record);
        }
    }
    let states = live(home, &records)?;
    let mut machines = Vec::new();
    for (record, state) in records.iter().zip(states) {
        machines.push(summary(home, record, state)?);
    }
    Ok(SdkValue::map([("machines", SdkValue::List(machines))]))
}
