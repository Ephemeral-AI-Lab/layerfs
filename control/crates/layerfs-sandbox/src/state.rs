//! A Sandbox's state (call surface 2.0): one of the six state words with
//! its companion, derived from its record and its machine's record with no
//! probe, so `sandbox list`, `sandbox get`, the home's tree, the Sandbox
//! doctor and the Workspace calls all say the same thing of it.
//!
//! In order: a live owner's `op` is `busy` (`create` or `remove`); a gone
//! owner's uncertain `op`, or a machine stop whose outcome is not known, is
//! `broken` `unresolved`; a record with no runtime id, or a runtime a
//! confirmed stop ended, is `broken` `missing`; a machine that is not
//! ready makes it `blocked`; a record of the retired docker backend is
//! `broken` `unsupported`; a `reason` a live read recorded (`lost`,
//! `not_answering`) is `broken` with it; anything else is `ready`. Every
//! `fix` is `layerfs sandbox remove <sandbox>`. `unknown` is never derived
//! from records: only `sandbox get` answers it, when its machine does not
//! answer.
//!
//! A Workspace's state comes from its daemon's `activity` ([`activity`]).
use super::record::SandboxRecord;
use layerfs_protocol::provision::install::SdkRefusal;
use layerfs_home::op::Standing;
use layerfs_home::outcome::shell_word;
use layerfs_home::state::{Reason, State};
use layerfs_machine::record::MachineRecord;
use std::path::Path;

/// The reasons a live read writes on a Sandbox's record, the only ones a
/// record holds.
pub const OBSERVED: [Reason; 2] = [Reason::Lost, Reason::NotAnswering];

/// The `fix` of every broken Sandbox.
pub fn remove_fix(sandbox: &str) -> String {
    format!("layerfs sandbox remove {}", shell_word(sandbox))
}

/// The reason a record's `reason` word names, when a record may hold it.
fn observed(word: &str) -> Option<Reason> {
    OBSERVED.into_iter().find(|reason| reason.word() == word)
}

impl SandboxRecord {
    /// The Sandbox's state as its record says it, with its machine's state
    /// `machine` (records only: [`machine_state`]). See the module's doc for
    /// the order.
    pub fn status(&self, machine: &State) -> State {
        let broken = |reason| State::broken(reason, Some(&remove_fix(self.name())));
        if let Some(op) = self.op() {
            match op.standing() {
                Standing::Running => {
                    return State::Busy {
                        name: op.word().to_owned(),
                        phase: op.phase().to_owned(),
                        started_ms: op.started_ms(),
                    }
                }
                Standing::Unresolved => return broken(Reason::Unresolved),
                Standing::Stale => {}
            }
        }
        if self.machine_stop_unknown() {
            return broken(Reason::Unresolved);
        }
        if self.machine_stopped() || self.id().is_none() {
            return broken(Reason::Missing);
        }
        // a retired Sandbox's remove asks its machine's worker to forget it:
        // a machine that is not ready blocks it first
        if let Some(blocked) = machine.blocks("machine", self.machine()) {
            return blocked;
        }
        if self.retired() {
            return broken(Reason::Unsupported);
        }
        match self.reason_word().and_then(observed) {
            Some(reason) => broken(reason),
            None => State::Ready,
        }
    }

    /// The sentence behind a `blocked` or `broken` state, the long form's
    /// `message`; none for any other.
    pub fn message(&self, state: &State) -> Option<String> {
        match state {
            State::Blocked(by) => Some(match by.reason {
                "stopped" | "busy" => format!("{} {} is {}", by.thing, by.handle, by.reason),
                reason => format!("{} {} is broken ({reason})", by.thing, by.handle),
            }),
            State::Broken { reason, .. } => Some(self.why(*reason)),
            _ => None,
        }
    }

    /// Why the Sandbox is broken for `reason`, as a sentence.
    fn why(&self, reason: Reason) -> String {
        match reason {
            Reason::Unresolved => match self.op() {
                Some(op) => format!(
                    "the outcome of its {} (phase {}) is not known",
                    op.word(),
                    op.phase()
                ),
                None => "a stop of its runtime was attempted and its outcome is not known".into(),
            },
            Reason::Missing if self.machine_stopped() => "its runtime was stopped".to_owned(),
            Reason::Missing => "it has no runtime: its create never recorded one".to_owned(),
            Reason::Unsupported => "it was started by the retired docker backend; removing it \
                                    removes its records and leaves its container"
                .to_owned(),
            Reason::Lost => (self.error())
                .unwrap_or("its machine says it does not run")
                .to_owned(),
            Reason::NotAnswering => (self.error())
                .unwrap_or("its daemon admits no connection")
                .to_owned(),
            other => other.word().to_owned(),
        }
    }
}

/// The state of machine `name` from its record alone (what the control
/// crate's inventory answers; that crate is above this one): `native` and
/// `docker` exist with or without a record, any
/// other machine with none is `broken` `missing`, and one whose record
/// cannot be read `broken` `failed`. The fix of an outdated
/// worker, which counts the machine's Sandboxes, is not needed here: a
/// Sandbox's `blocked_by` carries only the reason.
pub fn machine_state(home: &Path, name: &str) -> Result<State, SdkRefusal> {
    let record = match MachineRecord::read(home, name) {
        Ok(Some(record)) => record,
        Ok(None) => return Ok(layerfs_machine::state::unrecorded(name)),
        // a record that cannot be read blocks what is on it, nothing else
        Err(_) => return Ok(layerfs_machine::state::unreadable(name)),
    };
    let platform = record.platform_name().unwrap_or("");
    let removed = layerfs_machine::table::removed::removed_reason(platform).is_some();
    Ok(record.status(removed, 0))
}

/// Sandbox `record`'s state from records: its own and its machine's.
pub fn state(home: &Path, record: &SandboxRecord) -> Result<State, SdkRefusal> {
    Ok(record.status(&machine_state(home, record.machine())?))
}

/// The `fix` of a broken Workspace.
pub fn unmount_fix(workspace: &str) -> String {
    format!(
        "layerfs workspace unmount {workspace} --discard --force",
        workspace = shell_word(workspace)
    )
}

/// The state of Workspace `workspace` (its handle) from its daemon's
/// `activity`: `idle` is `ready`; `attaching`, `committing` and `closing`
/// are `busy` with `op.name` `mount`, `commit` and `unmount` (the daemon
/// records no start time for them); `uncertain` is `broken` `unresolved`
/// and a local failure (`failed`, `local_failure`) `broken` `failed`, each
/// fixed by a discarding, forced unmount. A word this build does not know
/// is `unknown`.
pub fn activity(activity: &str, workspace: &str) -> State {
    let busy = |name: &str| State::Busy {
        name: name.to_owned(),
        phase: activity.to_owned(),
        started_ms: None,
    };
    let broken = |reason| State::broken(reason, Some(&unmount_fix(workspace)));
    match activity {
        "idle" => State::Ready,
        "attaching" => busy("mount"),
        "committing" => busy("commit"),
        "closing" => busy("unmount"),
        "uncertain" => broken(Reason::Unresolved),
        "failed" | "local_failure" => broken(Reason::Failed),
        _ => State::Unknown { observed_ms: None },
    }
}
