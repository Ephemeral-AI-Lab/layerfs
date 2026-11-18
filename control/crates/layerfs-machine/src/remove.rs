//! `machine.remove`, which only forgets a machine of a platform removed in
//! 2.0. Its phases are `check` and `delete` (call surface 2.0), each a
//! `{ phase, done, total }` event on the caller's stream; it answers `{ ok,
//! forgotten, outcome, machine, message }`.
use super::progress::Phases;
use super::record::{MachineRecord, REMOVE};
use super::spec::home_id;
use super::spec::{self as record, refusal};
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{lock, paths};
use layerfs_home::names::MachineName;
use layerfs_home::op::Op;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_platform::{self as adapters, Code};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The phases of a remove.
const PHASES: [&str; 2] = ["check", "delete"];

/// What a remove answers: `forgotten` for a machine of a removed platform,
/// whose `message` says what was left on its host.
fn removed(name: &str, forgotten: Option<String>) -> SdkValue {
    let outcome = if forgotten.is_some() {
        "forgotten"
    } else {
        "removed"
    };
    SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("forgotten", SdkValue::Bool(forgotten.is_some())),
        ("outcome", SdkValue::text(outcome)),
        ("machine", SdkValue::text(name)),
        ("message", SdkValue::opt_text(forgotten)),
    ])
}

/// The home lock covers only the checks and the record writes: the record
/// holds this call's `op` while the VM is deleted under the machine's start
/// lock, so no call starts it, moves to it or runs on it meanwhile. It is
/// refused `Busy` while a live process holds the machine; it clears a gone
/// one's unresolved intent. A refused delete puts the record back as it
/// was; one whose outcome is unknown keeps its `op`, uncertain, with the
/// `error`, and is logged.
fn take_down(
    home: &Path,
    name: &MachineName,
    related: Related,
    events: &mut Events<'_>,
) -> Outcome {
    let home = home.to_path_buf();
    let mut phases = Phases::new(events, &PHASES);
    phases.begin("check");
    let record = MachineRecord::of(&home, name)?;
    record.not_running()?;
    if let Some(reason) = record.removed() {
        let answer = forget(&home, name, reason, related)?;
        phases.end();
        return Ok(answer);
    }
    let _start = super::endpoint::start_lock(&home, name)?;
    let (record, spec, op) = {
        let _lock = lock::take(&home)?;
        let (record, spec) = removable(&home, name, related)?;
        let op = Op::begin(REMOVE, "delete", SdkValue::Null)?;
        record.clone().with_op(&op).write(&home)?;
        (record, spec, op)
    };
    phases.begin("delete");
    if let Err(error) = adapters::remove(&spec) {
        // an unknown outcome says so in the record; a refused one leaves it as it was
        let _lock = lock::take(&home)?;
        if error.code == Code::Unknown {
            let op = op.as_uncertain();
            let message = SdkValue::text(&error.message);
            record.with_op(&op).with("error", message).write(&home)?;
            op.log_uncertain(&home, "machine", name, &error.message);
        } else {
            record.write(&home)?;
        }
        return Err(refusal(error));
    }
    let _lock = lock::take(&home)?;
    std::fs::remove_dir_all(paths::machine_dir(&home, name)).map_err(failed)?;
    layerfs_home::home::generation::moved(&home);
    phases.end();
    Ok(removed(name, None))
}

/// Forgets a machine whose platform was removed in 2.0: its record goes,
/// under the home lock and the same `InUse` check as a remove, and nothing
/// outside the home is touched; the answer's `note` says what is left.
fn forget(home: &Path, name: &MachineName, reason: &str, related: Related) -> Outcome {
    let _lock = lock::take(home)?;
    let record = MachineRecord::of(home, name)?;
    record.not_running()?;
    (related.can_remove)(home, name)?;
    let vm = || -> Result<String> { Ok(record.provider_name(&home_id(home)?)) };
    let left = match record.platform_name() {
        Some("microsandbox") => {
            let vm = vm()?;
            format!(
                "its VM {vm} and data volume {vm}-data were not touched; \
                 `msb remove --force {vm}` and `msb volume remove {vm}-data` remove them"
            )
        }
        Some("firecracker") => {
            let vm = vm()?;
            let host = match record.text("host") {
                Some(host) => format!("Lima machine {host}"),
                None => "its Lima host".to_owned(),
            };
            format!(
                "its microVM {vm} was not touched: its folder \
                 /layerfs/data/firecracker/vms/{vm} and data disk \
                 /layerfs/data/firecracker/disks/{vm}.ext4 stay in {host}"
            )
        }
        _ => "nothing on its host was touched".to_owned(),
    };
    std::fs::remove_dir_all(paths::machine_dir(home, name)).map_err(failed)?;
    layerfs_home::home::generation::moved(home);
    let left = format!("forgot the record only: {reason}; {left}");
    Ok(removed(name, Some(left)))
}

/// The checks before a remove, under the home lock: the machine's record
/// and spec.
fn removable(home: &Path, name: &str, related: Related) -> Result<(MachineRecord, adapters::Spec)> {
    let record = MachineRecord::of(home, name)?;
    record.not_running()?;
    let kind = record.platform()?;
    if kind.built_in() {
        return Err(refused(
            "InvalidInput",
            format!("{name} is built in and never removed"),
        ));
    }
    (related.can_remove)(home, name)?;
    let spec = record::spec(home, &home_id(home)?, &record)?;
    Ok((record, spec))
}

/// `machine.remove(name)`: deletes a VM and its disk; a machine of a
/// platform removed in 2.0 is only forgotten.
pub fn remove(
    home: &Path,
    name: &MachineName,
    related: Related,
    events: &mut Events<'_>,
) -> Outcome {
    take_down(home, name, related, events)
}
