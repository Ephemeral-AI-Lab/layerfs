//! `machine.remove`. Its phases are `check` and `delete` (call surface
//! 2.0), each a `{ phase, done, total }` event on the caller's stream; it
//! answers `{ ok, outcome, machine, message }`.
use super::progress::Phases;
use super::record::{MachineRecord, REMOVE};
use super::spec::{self as record, refusal};
use crate::Related;
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{lock, paths};
use layerfs_home::names::MachineName;
use layerfs_home::op::Op;
use layerfs_home::outcome::{failed, refused, Outcome};
use crate::platform::{self as adapters, Code};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The phases of a remove.
const PHASES: [&str; 2] = ["check", "delete"];

/// What a remove answers; `message` says when only the record went.
fn removed(name: &str, message: Option<String>) -> SdkValue {
    SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("outcome", SdkValue::text("removed")),
        ("machine", SdkValue::text(name)),
        ("message", SdkValue::opt_text(message)),
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
        let answer = drop_record(&home, name, reason, related)?;
        phases.end();
        return Ok(answer);
    }
    // a machine whose record has no `vm_name` (an earlier build's): this
    // build cannot name its VM or container, so only the record goes; a
    // built-in `docker` is recorded again, with its 2.0 name, by the next
    // call. `native` has no provider name and is built in: it goes on to
    // the built-in refusal below
    if record.nameless() {
        let reason = match record.platform()?.built_in() {
            true => "its record has no vm_name; the next call records it again",
            false => "its record has no vm_name",
        };
        let answer = drop_record(&home, name, reason, related)?;
        super::spec::forget_built_ins(&home);
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

/// Removes the record of a machine whose platform was removed in 2.0, or
/// whose record has no `vm_name`, under the home lock and the same `InUse`
/// check as a remove (whose `fix` then forgets the Stores and Sandboxes
/// recorded on it first); nothing outside the home is touched, and
/// `message` says so.
fn drop_record(home: &Path, name: &MachineName, reason: &str, related: Related) -> Outcome {
    let _lock = lock::take(home)?;
    let record = MachineRecord::of(home, name)?;
    record.not_running()?;
    (related.can_remove)(home, name)?;
    std::fs::remove_dir_all(paths::machine_dir(home, name)).map_err(failed)?;
    layerfs_home::home::generation::moved(home);
    let message =
        format!("only its record was removed ({reason}); nothing on its host was touched");
    Ok(removed(name, Some(message)))
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
    let spec = record::spec(home, &record)?;
    Ok((record, spec))
}

/// `machine.remove(name)`: deletes a VM and its disk; of a machine of a
/// platform removed in 2.0 only the record goes.
pub fn remove(
    home: &Path,
    name: &MachineName,
    related: Related,
    events: &mut Events<'_>,
) -> Outcome {
    take_down(home, name, related, events)
}
