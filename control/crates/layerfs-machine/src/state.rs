//! A machine's state from its record alone (call surface 2.0): one of the
//! six state words with its companion, derived with no probe, so
//! `machine list`, `machine get`, the home's tree, a Store's or Sandbox's
//! `blocked_by` and the machine doctor all say the same thing of it.
//!
//! The record holds what a lifecycle call or a live read last found: the
//! `op` of a create, start, stop or remove in flight; `state` (`running`
//! or `stopped`); and,
//! while the machine is broken, `reason` (a [`Reason`] word) with the
//! sentence behind it in `error`. In order: a live owner's `op` is `busy`;
//! a gone owner's uncertain `op` is `broken` `unresolved`; a gone owner's
//! certain `op` is ignored (a read writes nothing); a platform removed in
//! 2.0 is `broken` `unsupported`; a machine whose platform has no provider
//! (`native`) is always `ready`; a record
//! with no `vm_name` (an earlier build's) is `broken` `outdated` with the
//! fix `machine remove`, after the removes of what is recorded on it
//! ([`MachineRecord::status_held`]); a recorded `reason` is `broken` with it; then
//! `running` is `ready` and anything else `stopped`. `unknown` is never
//! recorded: only a live read that could not be made answers it.
//!
//! Built without the `machine` feature too: the caller says whether the
//! record's platform was removed (`layerfs-control`'s
//! `inventory::machine_state` asks the platform policy).
use super::record::{Holders, MachineRecord, CREATE, REMOVE};
use layerfs_home::op::{Op, Standing};
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::{with_fix, with_next};
use layerfs_home::state::{Reason, State};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};

/// The record field that holds a broken machine's reason code.
pub const REASON: &str = "reason";

/// The reason codes a machine's record may hold.
const RECORDED: [Reason; 6] = [
    Reason::EngineDown,
    Reason::NotAnswering,
    Reason::Outdated,
    Reason::Missing,
    Reason::Failed,
    Reason::Unsupported,
];

/// The reason `word` names, when a machine's record may hold it.
pub fn reason_of(word: &str) -> Option<Reason> {
    RECORDED.into_iter().find(|reason| reason.word() == word)
}

/// The host's own command that starts Docker: the `fix` of `engine_down`.
pub fn start_docker() -> &'static str {
    match cfg!(target_os = "macos") {
        true => "open -a Docker",
        false => "sudo systemctl start docker",
    }
}

/// The `fix` of a worker of another build (ruling R2, review fix 5): a
/// stop and a start, the stop forced when Sandboxes are recorded on the
/// machine (nothing can be sent through the old worker to remove them).
pub fn replace_worker(machine: &str, sandboxes: usize) -> String {
    let force = if sandboxes > 0 { " --force" } else { "" };
    format!(
        "layerfs machine stop {machine}{force} && layerfs machine start {machine}",
        machine = shell_word(machine)
    )
}

/// The call that settles an unresolved `op`: a create or remove is cleared
/// by `machine remove`; a start or stop by a live read, which observes the
/// machine (correction C13).
pub fn settle(machine: &str, op: &Op) -> String {
    match op.name() {
        CREATE | REMOVE => format!(
            "layerfs machine remove {machine}",
            machine = shell_word(machine)
        ),
        _ => format!(
            "layerfs machine get {machine} --with live",
            machine = shell_word(machine)
        ),
    }
}

/// The one runnable line that fixes a machine broken for `reason`, when
/// there is one.
fn fix(machine: &str, reason: Reason, removed: bool, sandboxes: usize) -> Option<String> {
    match reason {
        Reason::EngineDown => Some(start_docker().to_owned()),
        // a start does not replace a worker that runs and does not answer
        Reason::Outdated | Reason::NotAnswering => Some(replace_worker(machine, sandboxes)),
        Reason::Failed => Some(format!(
            "layerfs machine start {machine}",
            machine = shell_word(machine)
        )),
        Reason::Missing => Some(format!(
            "layerfs machine remove {machine}",
            machine = shell_word(machine)
        )),
        Reason::Unsupported if removed => Some(format!(
            "layerfs machine remove {machine}",
            machine = shell_word(machine)
        )),
        _ => None,
    }
}

/// How many Sandboxes are on it, as a sentence's end.
pub(crate) fn count(sandboxes: usize) -> String {
    match sandboxes {
        1 => "1 Sandbox is on it".to_owned(),
        n => format!("{n} Sandboxes are on it"),
    }
}

impl MachineRecord {
    /// The reason code the record holds, while the machine is broken.
    pub fn reason(&self) -> Option<Reason> {
        self.text(REASON).and_then(reason_of)
    }

    /// The record as broken for `reason`, with the sentence behind it.
    pub fn broken_as(self, reason: Reason, message: &str) -> Self {
        let record = self.with(REASON, SdkValue::text(reason.word()));
        record.with("error", SdkValue::text(message))
    }

    /// Whether nothing on the machine can be reached again: its record has
    /// no 2.0 name (an earlier build's), or a live read found that its
    /// provider holds no VM of its name (`missing`, area 10). A Store or
    /// Sandbox recorded on it then goes by its records alone, and its fix
    /// removes those first, then the machine.
    pub fn gone(&self) -> bool {
        self.nameless() || (self.provided() && self.reason() == Some(Reason::Missing))
    }

    /// Refuses a call that needs the VM of a machine a live read found
    /// `missing`: `Failed`, its remove as the fix, which the call boundary
    /// widens to the chain of what is on it (as for a record with no
    /// `vm_name`). A live read still probes it, and mends it if the VM is back.
    pub fn vm_present(&self) -> Result<(), SdkRefusal> {
        if self.nameless() || !self.gone() {
            return Ok(());
        }
        let said = self
            .text("error")
            .unwrap_or("its provider holds no VM of its name");
        let message = format!(
            "machine {}'s VM is gone ({said}); only its records remain",
            self.name()
        );
        let fix = super::record::no_vm_name_fix(self.name(), Holders::default());
        Err(with_fix(
            layerfs_home::outcome::refused("Failed", message),
            fix,
        ))
    }

    /// The record with no reason and no `error`: the machine is not broken.
    pub fn mended(self) -> Self {
        self.without(REASON).with("error", SdkValue::Null)
    }

    /// The machine's state as its record says it, with no probe. `removed`
    /// says its platform was removed in 2.0; `sandboxes` is how many
    /// Sandboxes are recorded on it, which the `fix` of an outdated worker
    /// depends on. See the module's doc for the order.
    pub fn status(&self, removed: bool, sandboxes: usize) -> State {
        let name = self.name();
        if let Some(op) = self.op() {
            match op.standing() {
                Standing::Running => {
                    return State::Busy {
                        name: op.word().to_owned(),
                        phase: op.phase().to_owned(),
                        started_ms: op.started_ms(),
                    }
                }
                Standing::Unresolved => {
                    return State::broken(Reason::Unresolved, Some(&settle(name, &op)))
                }
                Standing::Stale => {}
            }
        }
        if removed {
            let fix = fix(name, Reason::Unsupported, true, sandboxes);
            return State::broken(Reason::Unsupported, fix.as_deref());
        }
        if name == layerfs_home::names::HOST_MACHINE && self.recorded_state() == Some("stopped") {
            return State::Stopped;
        }
        if !self.provided() {
            return State::Ready;
        }
        // a record an earlier build made, with no provider name: only its
        // record can go
        if self.nameless() {
            let fix = super::record::no_vm_name_fix(name, Holders::default());
            return State::broken(Reason::Outdated, Some(&fix));
        }
        if let Some(reason) = self.reason() {
            let fix = fix(name, reason, false, sandboxes);
            return State::broken(reason, fix.as_deref());
        }
        match self.recorded_state() {
            Some("running") => State::Ready,
            _ => State::Stopped,
        }
    }

    /// [`Self::status`] with what is recorded on the machine named
    /// (`held`): a record with no `vm_name`, or one whose VM a live read
    /// found `missing` ([`Self::gone`]), that still holds Sandboxes, Stores
    /// or old Firecracker records shows the line its remove's `InUse`
    /// refusal carries ([`super::record::no_vm_name_fix`]), not a bare
    /// remove that is refused.
    pub fn status_held(&self, removed: bool, held: Holders<'_>) -> State {
        match self.status(removed, held.sandboxes.len()) {
            State::Broken {
                reason: reason @ (Reason::Outdated | Reason::Missing),
                ..
            } if self.gone() => {
                let fix = super::record::no_vm_name_fix(self.name(), held);
                State::broken(reason, Some(&fix))
            }
            state => state,
        }
    }

    /// The sentence behind a broken machine's reason, the long form's
    /// `message`: the record's `error`; for an outdated worker with
    /// Sandboxes on the machine it names how many; for an unresolved
    /// operation it says its outcome is not known.
    pub fn message(&self, state: &State, sandboxes: usize) -> Option<String> {
        let State::Broken { reason, .. } = state else {
            return None;
        };
        if self.nameless() {
            return Some(super::record::no_vm_name(self.name()));
        }
        let said = self.text("error").map(str::to_owned);
        match reason {
            Reason::Outdated if sandboxes > 0 => {
                let said = said.unwrap_or_else(|| "its worker is from another build".to_owned());
                Some(format!("{said}; {}", count(sandboxes)))
            }
            Reason::Unresolved => {
                let op = self.op()?;
                let what = format!(
                    "the outcome of its {} (phase {}) is not known",
                    op.word(),
                    op.phase()
                );
                Some(said.map_or(what.clone(), |said| format!("{what}: {said}")))
            }
            _ => said,
        }
    }

    /// Refuses a new operation on the machine while `op` holds it: `Busy`
    /// while its owner lives, `Unresolved` once a gone owner left its
    /// outcome unknown, with the call that settles it as `fix`. Either
    /// names the `op` for the caller's next step.
    pub(crate) fn held_by(&self, op: &Op) -> Result<(), SdkRefusal> {
        let name = self.name();
        let Err(mut refusal) = op.blocks("machine", name, "machine get", "machine remove") else {
            return Ok(());
        };
        if refusal.code == "Unresolved" {
            let fix = settle(name, op);
            if !matches!(op.name(), CREATE | REMOVE) {
                refusal.message = format!(
                    "machine {name} has an unresolved {} ({}, phase {}): its outcome is not \
                     known; `{fix}` observes the machine and settles it",
                    op.word(),
                    op.id(),
                    op.phase()
                );
            }
            refusal = with_fix(refusal, fix);
        }
        Err(with_next(refusal, "op", op.shown()))
    }
}

/// The state of machine `name` when it has no record: a built-in machine
/// exists with or without one, ready when its platform (which has its
/// name) has no provider (`native`), else never started (`docker`); any
/// other machine a Store or Sandbox still names is `broken` `missing`.
pub fn unrecorded(name: &str) -> State {
    match layerfs_home::names::BUILT_IN.contains(&name) {
        true if layerfs_home::names::built_in_platform(name) == Some("host") => State::Ready,
        true => State::Stopped,
        false => State::broken(Reason::Missing, None),
    }
}

/// The state of machine `name` whose record is there but cannot be read:
/// `broken` `failed`, its doctor (which names the record, `machine.record`)
/// as the fix. A list shows it as one item; a Store or Sandbox on it is
/// blocked by it; nothing else fails for it (RF re-check N-D).
pub fn unreadable(name: &str) -> State {
    State::broken(Reason::Failed, Some(&unreadable_fix(name)))
}

/// The fix of machine `name` whose record cannot be read.
pub fn unreadable_fix(name: &str) -> String {
    format!("layerfs machine doctor {name}", name = shell_word(name))
}
