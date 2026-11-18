//! The one state of a machine, Store, project, Sandbox or Workspace (call
//! surface 2.0, C11): six words, each with its companion keys and no other.
//! A result carries `state` and the companions of that state; the long form
//! of a call lists every companion key, so the others answer null there.
use crate::home::with_field;
use crate::op::Op;
use layerfs_bridge::control::SdkValue;

/// Why a thing is broken, as a code; the sentence is the long form's
/// `message`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Its container engine is not running.
    EngineDown,
    /// It runs and does not answer.
    NotAnswering,
    /// It is from another build or an older layout.
    Outdated,
    /// It cannot be reached over the network.
    Unreachable,
    /// What its record names is not there.
    Missing,
    /// Its last operation failed.
    Failed,
    /// A gone process left an operation on it whose outcome is not known.
    Unresolved,
    /// It ended behind LayerFS's back.
    Lost,
    /// This build or host does not support it.
    Unsupported,
}

impl Reason {
    /// Every reason, in the order the specification lists them.
    pub const ALL: [Reason; 9] = [
        Reason::EngineDown,
        Reason::NotAnswering,
        Reason::Outdated,
        Reason::Unreachable,
        Reason::Missing,
        Reason::Failed,
        Reason::Unresolved,
        Reason::Lost,
        Reason::Unsupported,
    ];

    /// The reason as results show it.
    pub fn word(self) -> &'static str {
        match self {
            Reason::EngineDown => "engine_down",
            Reason::NotAnswering => "not_answering",
            Reason::Outdated => "outdated",
            Reason::Unreachable => "unreachable",
            Reason::Missing => "missing",
            Reason::Failed => "failed",
            Reason::Unresolved => "unresolved",
            Reason::Lost => "lost",
            Reason::Unsupported => "unsupported",
        }
    }
}

/// The thing at the root of a block: its own word (`machine`, `store`,
/// `sandbox`), its handle and why it blocks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockedBy {
    /// The blocker's own word: `machine`, `store` or `sandbox`.
    pub thing: &'static str,
    /// Its handle.
    pub handle: String,
    /// The blocker's state (`stopped`, `busy`) or its broken reason.
    pub reason: &'static str,
}

impl BlockedBy {
    /// `{ <thing>: handle, reason }`, as results show it.
    pub fn to_sdk(&self) -> SdkValue {
        SdkValue::map([
            (self.thing, SdkValue::text(&self.handle)),
            ("reason", SdkValue::text(self.reason)),
        ])
    }
}

/// The state of one thing, with its companion.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// It can be used.
    Ready,
    /// A live process runs an operation on it: `op { name, phase, started_at }`.
    Busy {
        /// The operation's call word, as its record holds it.
        name: String,
        /// The phase it is in.
        phase: String,
        /// When it began, in milliseconds since 1970.
        started_ms: Option<i64>,
    },
    /// It was stopped on purpose.
    Stopped,
    /// Something it needs is not ready: `blocked_by { <thing>, reason }`.
    Blocked(BlockedBy),
    /// It cannot be used until it is fixed: `reason` and `fix`.
    Broken {
        /// Why, as a code.
        reason: Reason,
        /// One runnable shell line (two commands joined with `&&`), if any.
        fix: Option<String>,
    },
    /// LayerFS cannot tell: `observed_at`.
    Unknown {
        /// When LayerFS last knew its state, in milliseconds since 1970.
        observed_ms: Option<i64>,
    },
}

/// The six state words, in the order the specification lists them.
pub const WORDS: [&str; 6] = ["ready", "busy", "stopped", "blocked", "broken", "unknown"];

impl State {
    /// `busy` with the recorded operation `op`.
    pub fn busy(op: &Op) -> Self {
        State::Busy {
            name: op.name().to_owned(),
            phase: op.phase().to_owned(),
            started_ms: op.to_record().get_int("started"),
        }
    }

    /// `broken` for `reason`, with its fix line when there is one.
    pub fn broken(reason: Reason, fix: Option<&str>) -> Self {
        State::Broken {
            reason,
            fix: fix.map(str::to_owned),
        }
    }

    /// `blocked` by the `thing` (`machine`, `store`, `sandbox`) named
    /// `handle`, for `reason`: that thing's state or its broken reason.
    pub fn blocked_by(thing: &'static str, handle: &str, reason: &'static str) -> Self {
        State::Blocked(BlockedBy {
            thing,
            handle: handle.to_owned(),
            reason,
        })
    }

    /// The state of something that needs this `thing` named `handle`:
    /// blocked by it unless it is ready. A thing blocked itself passes its
    /// own root on, so `blocked_by` always names the root of the block; one
    /// whose state is unknown blocks nothing (the thing below answers for
    /// itself).
    pub fn blocks(&self, thing: &'static str, handle: &str) -> Option<State> {
        match self {
            State::Ready | State::Unknown { .. } => None,
            State::Busy { .. } => Some(State::blocked_by(thing, handle, "busy")),
            State::Stopped => Some(State::blocked_by(thing, handle, "stopped")),
            State::Blocked(root) => Some(State::Blocked(root.clone())),
            State::Broken { reason, .. } => Some(State::blocked_by(thing, handle, reason.word())),
        }
    }

    /// The state's word.
    pub fn word(&self) -> &'static str {
        match self {
            State::Ready => "ready",
            State::Busy { .. } => "busy",
            State::Stopped => "stopped",
            State::Blocked(_) => "blocked",
            State::Broken { .. } => "broken",
            State::Unknown { .. } => "unknown",
        }
    }

    /// `state` and this state's companion keys, as a result holds them.
    pub fn fields(&self) -> Vec<(&'static str, SdkValue)> {
        let mut fields = vec![("state", SdkValue::text(self.word()))];
        let at =
            |ms: &Option<i64>| ms.map_or(SdkValue::Null, |ms| SdkValue::Text(crate::time::iso(ms)));
        match self {
            State::Ready | State::Stopped => {}
            State::Busy {
                name,
                phase,
                started_ms,
            } => fields.push((
                "op",
                SdkValue::map([
                    ("name", SdkValue::text(name)),
                    ("phase", SdkValue::text(phase)),
                    ("started_at", at(started_ms)),
                ]),
            )),
            State::Blocked(by) => fields.push(("blocked_by", by.to_sdk())),
            State::Broken { reason, fix } => {
                fields.push(("reason", SdkValue::text(reason.word())));
                fields.push(("fix", fix.as_ref().map_or(SdkValue::Null, SdkValue::text)));
            }
            State::Unknown { observed_ms } => fields.push(("observed_at", at(observed_ms))),
        }
        fields
    }

    /// Result map `value` with `state` and this state's companions set.
    pub fn write(&self, mut value: SdkValue) -> SdkValue {
        for (name, field) in self.fields() {
            value = with_field(value, name, field);
        }
        value
    }
}
