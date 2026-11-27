//! `machine stop --force`: an explicit provider crash stop without a
//! worker request, VM start or data deletion. Its phases are `check` and
//! `stop`, as a plain stop's; it answers what a stop does, with `forced`,
//! `stopped_sandboxes` and `unknown_workspaces` (call surface 2.0). The
//! stop of the Sandboxes on the machine is the Sandbox crate's
//! ([`MachineUsers::force_stop`]); this file takes the machine's lock and
//! checks its record.
use crate::record::machine::MachineRecord;
use crate::{MachineUsers, Phases, STOP_PHASES};
use layerfs_home::names::MachineName;
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkRefusal;
use std::path::Path;
type Result<T> = std::result::Result<T, SdkRefusal>;

pub(crate) fn stop(
    home: &Path,
    events: &mut Events<'_>,
    name: &MachineName,
    users: &MachineUsers,
) -> Outcome {
    let name = name.as_str();
    let mut phases = Phases::new(events, &STOP_PHASES);
    phases.begin("check");
    force_record(home, name)?;
    let _start = crate::force_stop_lock(home, name)?;
    (users.force_stop)(home, name, phases)
}

pub(crate) fn host_locked(
    home: &Path,
    events: &mut Events<'_>,
    name: &MachineName,
    users: &MachineUsers,
) -> Outcome {
    let mut phases = Phases::new(events, &STOP_PHASES);
    phases.begin("check");
    (users.force_stop)(home, name.as_str(), phases)
}

/// A new explicit force call may reconcile an unknown stop that this
/// process left, or one whose owner has ended (`Unresolved`); one a live
/// process still holds stays `Busy`. It never silently retries, nor clears
/// another kind of unresolved operation.
pub fn force_record(home: &Path, name: &str) -> Result<MachineRecord> {
    let record = MachineRecord::of(home, name)?;
    if record.op().is_some_and(|op| {
        op.name() == "machine.stop" && op.uncertain() && (op.mine() || !op.owner_alive())
    }) {
        return Ok(record);
    }
    record.not_held()?;
    Ok(record)
}
