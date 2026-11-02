//! `machine.list` and the live probes and summaries it shares with `get`, `update` and the create/connect outcome.
use super::record::{self, backend_name, kind_of, refusal, removing, sandboxes_on, stores_on};
use super::status::home_id;
use crate::common::context::Context;
use crate::common::home::{paths, read_record};
use crate::common::outcome::{failed, Outcome};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_machine as adapters;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// How long `machine.list` and `machine.get` wait for their live probes, all
/// together. A probe that has not answered by then is `unknown`.
const PROBE: Duration = Duration::from_secs(10);

/// A machine's live state and, when it could not be read, why.
type Live = Option<(String, Option<String>)>;

/// The live state of each VM and ssh machine, read as `machine.status`
/// reads it: in parallel, starting nothing, outside the home lock, all within
/// [`PROBE`]. A probe that fails or does not answer in time is `unknown`, with
/// the reason; a command it still runs then is killed with its process group
/// and reaped by the probe's thread. Nothing is written back. `None` keeps
/// the record's state: a built-in machine, or one being created or removed.
pub(super) fn live(home: &Path, records: &[(String, SdkValue)]) -> Result<Vec<Live>> {
    let id = home_id(home)?;
    let (send, answers) = mpsc::channel();
    let late = format!("its probe did not answer within {} s", PROBE.as_secs());
    let mut out: Vec<Live> = vec![None; records.len()];
    let mut waiting = 0;
    let until = Instant::now() + PROBE;
    // the probes' commands are stopped just after the list stops waiting,
    // so a late probe is reported as late, not as its stopped command
    let stop = until + Duration::from_millis(200);
    for (index, (name, record)) in records.iter().enumerate() {
        let in_flight = record.get_text("state") == Some("creating") || removing(record).is_some();
        if kind_of(record)?.built_in() || in_flight {
            continue;
        }
        match record::spec(home, &id, name, record) {
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

pub(super) fn summary(home: &Path, name: &str, record: &SdkValue, live: Live) -> Result<SdkValue> {
    let kind = kind_of(record)?;
    let (state, error) = match live {
        Some((state, error)) => (Some(state), error),
        None => (record.get_text("state").map(str::to_owned), None),
    };
    let list = |items: Vec<String>| SdkValue::List(items.into_iter().map(SdkValue::Text).collect());
    let mut fields = vec![
        ("name", SdkValue::text(name)),
        ("platform", SdkValue::text(kind.name())),
        ("backend", SdkValue::text(backend_name(kind))),
        ("supports", super::platforms::supports(kind)),
        (
            "state",
            SdkValue::text(state.as_deref().unwrap_or(if kind.built_in() {
                "ready"
            } else {
                "stopped"
            })),
        ),
        ("cpus", record.get("cpus").cloned().unwrap_or_default()),
        ("memory", record.get("memory").cloned().unwrap_or_default()),
        ("disk", record.get("disk").cloned().unwrap_or_default()),
        ("stores", list(stores_on(home, name)?)),
        ("sandboxes", list(sandboxes_on(home, name)?)),
    ];
    for extra in ["image", "host", "nested", "docker", "wsl", "error"] {
        if let Some(value) = record
            .get(extra)
            .filter(|_| extra != "error" || error.is_none())
        {
            fields.push((extra, value.clone()));
        }
    }
    if let Some(error) = error {
        fields.push(("error", SdkValue::Text(error)));
    }
    if let Some(ssh) = record.get("ssh") {
        fields.push(("ssh", ssh.clone()));
    }
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}

/// `machine.list()`: every machine with its live state and what it
/// supports; `machine.platforms` answers what could be created here.
pub fn list(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    let home = &context.home;
    let mut records = Vec::new();
    for name in paths::names(&home.join("machine")).map_err(failed)? {
        if let Some(record) = read_record(&paths::machine_record(home, &name))? {
            records.push((name, record));
        }
    }
    let states = live(home, &records)?;
    let mut machines = Vec::new();
    for ((name, record), state) in records.iter().zip(states) {
        machines.push(summary(home, name, record, state)?);
    }
    Ok(SdkValue::map([("machines", SdkValue::List(machines))]))
}
