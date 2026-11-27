//! `machine.stop` (#535 P1): stops the worker and, where there is one,
//! the VM; its disks stay. Built-in `native` answers at once. A call that
//! had nothing to do answers `unchanged: true` and records nothing (call
//! surface 2.0); one that works records its `op` and reports its phases.
use super::boot::start_lock;
use super::progress::Phases;
use super::start::done;
use crate::platform::{self as adapters, Code, Platform};
use crate::record::adapter_spec::{self as record, machine_of, refusal};
use crate::record::machine::{MachineRecord, STOP};
use crate::status::state::settle;
use crate::Related;
use layerfs_home::disk::paths;
use layerfs_home::lock::home;
use layerfs_home::names::MachineName;
use layerfs_home::record::intent::Op;
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::{failed, with_fix, Outcome};
use layerfs_home::reply::state::State;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long a stop waits for the provider to say whether a machine its
/// record calls stopped is.
pub(super) const CHECK: Duration = Duration::from_secs(10);

/// The phases of a stop.
pub const STOP_PHASES: [&str; 2] = ["check", "stop"];

/// Whether machine `record` describes is stopped already: its record says
/// so, no worker is recorded for it, and its provider agrees. A provider
/// that cannot be asked answers no: the stop then runs and says why.
fn stopped(home: &Path, machine: &MachineRecord) -> bool {
    let name = machine.name();
    if machine.recorded(0) != State::Stopped || paths::worker_record(home, name).exists() {
        return false;
    }
    let spec = record::provider_spec(home, machine);
    let seen = spec.map(|spec| adapters::provider_within(&spec, Instant::now() + CHECK));
    matches!(
        seen,
        Ok(Ok(adapters::Status {
            state: adapters::State::Stopped | adapters::State::Missing,
            ..
        }))
    )
}

/// `machine.stop(machine)`: refused `InUse` while Sandboxes are on it,
/// naming each; else its worker and VM stop under its `start.lock`, so no
/// `machine.start` runs meanwhile, and `worker.json` goes. Its disks and
/// Stores stay. A machine that is stopped already answers `unchanged:
/// true`. A lost reply keeps the `op`, uncertain: `machine get <m> --with
/// live` settles it.
pub fn stop(home: &Path, name: &MachineName, related: Related, events: &mut Events<'_>) -> Outcome {
    let home = home.to_path_buf();
    let machine = machine_of(&home, name)?;
    if machine.platform()? == Platform::Host || stopped(&home, &machine) {
        return Ok(done(name, None));
    }
    let mut phases = Phases::new(events, &STOP_PHASES);
    let _start = start_lock(&home, name)?;
    phases.begin("check");
    let (spec, op) = {
        let _lock = home::take(&home)?;
        let machine = machine_of(&home, name)?;
        (related.can_stop)(&home, name)?;
        let spec = record::spec(&home, &machine)
            .map_err(|r| related.nameless_fix(&home, name.as_str(), r))?;
        let op = Op::begin(STOP, "stop", SdkValue::Null)?;
        machine.with_op(&op).write(&home)?;
        (spec, op)
    };
    phases.begin("stop");
    let stopped = adapters::stop(&spec);
    let _lock = home::take(&home)?;
    match stopped {
        Ok(()) => {
            match std::fs::remove_file(paths::worker_record(&home, name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(failed(error)),
            }
            record::update(&home, name, |record| {
                let record = record.with("state", SdkValue::text("stopped"));
                record.mended().without_op()
            })?;
            phases.end();
            let done = done(name, Some("stopped"));
            Ok(layerfs_home::disk::with_field(
                done,
                "forced",
                SdkValue::Bool(false),
            ))
        }
        // a lost reply: whether it stopped is not known
        Err(error) if error.code == Code::Unknown => {
            let op = op.as_uncertain();
            record::update(&home, name, |record| {
                (record.with_op(&op)).with("error", SdkValue::text(&error.message))
            })?;
            op.log_uncertain(&home, "machine", name, &error.message);
            Err(with_fix(refusal(error), settle(name, &op)))
        }
        // refused: the machine is as it was
        Err(error) => {
            record::update(&home, name, MachineRecord::without_op)?;
            Err(refusal(error))
        }
    }
}

/// A forced stop's answer, with what it stopped beside the machine:
/// `forced`, `stopped_sandboxes` and `unknown_workspaces` (the Workspaces
/// whose uncommitted changes are then not known), each with its count. A
/// plain stop stops nothing beside the machine and answers none of them.
pub fn with_stopped(
    answer: SdkValue,
    forced: bool,
    sandboxes: &[String],
    workspaces: &[String],
) -> SdkValue {
    // each list keeps its first 20, its count only when it was cut
    let list = |items: &[String]| {
        layerfs_home::reply::values::shown(items.iter().map(SdkValue::text).collect(), 0, false)
    };
    let (sandboxes, workspaces) = (list(sandboxes), list(workspaces));
    let mut answer = answer;
    for (key, value) in [
        ("stopped_sandboxes", sandboxes.0),
        ("stopped_sandboxes_count", sandboxes.1),
        ("unknown_workspaces", workspaces.0),
        ("unknown_workspaces_count", workspaces.1),
        ("forced", SdkValue::Bool(forced)),
    ] {
        answer = layerfs_home::disk::with_field(answer, key, value);
    }
    answer
}
