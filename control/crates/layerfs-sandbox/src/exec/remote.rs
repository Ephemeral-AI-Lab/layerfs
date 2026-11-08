//! The command records this home keeps for a Sandbox whose daemon it
//! reaches directly (#535 G4): those commands run in its machine's worker,
//! which keeps no table of them (route `machine:<name>`), so a cancel rides
//! `sandbox.host` there (D-11) and a worker that does not answer gives
//! `unknown`. A worker that answers that the Sandbox does not run there
//! (`NoSuchSandbox`) confirms its commands ended with it: `ended`.
use super::cancel::{self, Outcome};
use super::record::ExecRecord;
use crate::host::Call;
use crate::record::SandboxRecord;
use crate::route::reach;
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::events::Events;
use std::path::Path;
use std::time::Duration;

/// Cancels `found`, a command this home records on Sandbox `record`'s
/// machine, in its worker: what it answers; `ended` when it answers that
/// the Sandbox does not run there; `unknown` when it is not reached, does
/// not answer or refuses otherwise. Nothing starts the machine.
pub fn remote(
    events: &mut Events<'_>,
    (home, record): (&Path, &SandboxRecord),
    found: &ExecRecord,
    grace: Duration,
) -> Outcome {
    let call = Call::ExecCancel {
        path: Some(found.path.clone()),
        exec_id: Some(found.id.clone()),
        grace_ms: Some(grace.as_millis() as i64),
        workspace: Some(found.workspace.clone()),
    };
    // a Sandbox reached directly runs on a machine: its worker answers
    match reach(home, events, record, call, crate::dispatch::call) {
        Ok(Ok(answer)) => Outcome::read(&answer),
        Ok(Err(refusal)) if refusal.code == "NoSuchSandbox" => Outcome::Ended,
        Ok(Err(_)) | Err(_) => Outcome::Unknown,
    }
}

/// What a close (`workspace` `None`) or an unmount of Sandbox `record`,
/// which this host reaches directly, does with the command records this
/// home keeps for it: `Busy`, or with `force` each cancelled first in its
/// worker.
pub fn settle(
    events: &mut Events<'_>,
    (home, record): (&Path, &SandboxRecord),
    workspace: Option<&str>,
    force: bool,
) -> Result<(), SdkRefusal> {
    let dir = record.dir(home);
    let mut remote = |found: &ExecRecord, grace| remote(events, (home, record), found, grace);
    cancel::settle(&dir, record.name(), workspace, force, &mut remote)
}
