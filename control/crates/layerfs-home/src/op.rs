//! The one in-flight intent (#536 §7.1, H4–H6): while a multi-step
//! operation runs, its entity's record holds one `op` with an id, the
//! operation's name and phase, the arguments status and doctor need, its
//! owner (`pid`, `pid_start`, `pid_boot`), when it started and last moved,
//! and whether its outcome could not be known.
//!
//! An operation writes `op` before its first effect, moves `phase` as it
//! goes, and on success writes its result and drops `op` in one record
//! write. A certain failure drops `op`; an uncertain one keeps it with
//! `uncertain: true` and appends a line to `log/uncertain.jsonl`. A live
//! owner blocks every other operation on the entity (`Busy`); a dead
//! owner's certain intent is ignored; a dead owner's uncertain intent
//! blocks with `Unresolved`, naming the call that shows it and the call that
//! clears it. Reading never clears an intent.
use crate::outcome::refused;
use crate::{home, process};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

/// The record field that holds the intent.
pub const FIELD: &str = "op";

/// One recorded intent, as its entity's record holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Op {
    fields: SdkValue,
}

/// What an intent means for a new operation on its entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    /// Its owner is alive: the operation still runs.
    Running,
    /// Its owner is gone and its outcome was not known.
    Unresolved,
    /// Its owner is gone after a certain outcome: it holds nothing.
    Stale,
}

impl Op {
    /// A new intent of this process for operation `name`, at `phase`, with
    /// `args` (a map of what status and doctor need; `Null` for none).
    pub fn begin(name: &str, phase: &str, args: SdkValue) -> Result<Self, SdkRefusal> {
        let now = SdkValue::Int(home::now_ms());
        let fields = SdkValue::map([
            ("id", SdkValue::text(mint()?)),
            ("name", SdkValue::text(name)),
            ("phase", SdkValue::text(phase)),
            ("args", args),
            ("owner", owner()),
            ("started", now.clone()),
            ("updated", now),
            ("uncertain", SdkValue::Bool(false)),
        ]);
        Ok(Self { fields })
    }

    /// The intent a record holds, if any.
    pub fn of(record: &SdkValue) -> Option<Self> {
        match record.get(FIELD) {
            Some(fields @ SdkValue::Map(_)) => Some(Self {
                fields: fields.clone(),
            }),
            _ => None,
        }
    }

    /// The intent's id, `op-<16 hex>`.
    pub fn id(&self) -> &str {
        self.fields.get_text("id").unwrap_or("")
    }

    /// The operation's name (`store.remove`, `sandbox.create`, …).
    pub fn name(&self) -> &str {
        self.fields.get_text("name").unwrap_or("")
    }

    /// The phase it last recorded.
    pub fn phase(&self) -> &str {
        self.fields.get_text("phase").unwrap_or("")
    }

    /// The arguments it recorded.
    pub fn args(&self) -> &SdkValue {
        static NULL: SdkValue = SdkValue::Null;
        self.fields.get("args").unwrap_or(&NULL)
    }

    /// The owner's process id.
    pub fn pid(&self) -> i64 {
        (self.fields.get("owner"))
            .and_then(|owner| owner.get_int("pid"))
            .unwrap_or(0)
    }

    /// Whether its outcome could not be known.
    pub fn uncertain(&self) -> bool {
        matches!(self.fields.get("uncertain"), Some(SdkValue::Bool(true)))
    }

    /// The intent moved to `phase`.
    pub fn at(self, phase: &str) -> Self {
        let fields = home::with_field(self.fields, "phase", SdkValue::text(phase));
        Self {
            fields: home::with_field(fields, "updated", SdkValue::Int(home::now_ms())),
        }
    }

    /// The intent marked uncertain.
    pub fn as_uncertain(self) -> Self {
        let fields = home::with_field(self.fields, "uncertain", SdkValue::Bool(true));
        Self {
            fields: home::with_field(fields, "updated", SdkValue::Int(home::now_ms())),
        }
    }

    /// Whether its owner is the process it names: by id, kernel start
    /// time and boot, so a reused id is not its owner.
    pub fn owner_alive(&self) -> bool {
        (self.fields.get("owner")).is_some_and(process::recorded_alive)
    }

    /// Whether this process owns it.
    pub fn mine(&self) -> bool {
        self.pid() == i64::from(std::process::id()) && self.owner_alive()
    }

    /// What it means for a new operation (see [`Standing`]).
    pub fn standing(&self) -> Standing {
        match (self.owner_alive(), self.uncertain()) {
            (true, _) => Standing::Running,
            (false, true) => Standing::Unresolved,
            (false, false) => Standing::Stale,
        }
    }

    /// Refuses a new operation on entity `kind` `entity` (`store`,
    /// `machine` or `sandbox`) while this intent holds it: `Busy` while its
    /// owner lives, `Unresolved` once a gone owner left it uncertain, naming
    /// `show` (the call that shows it, such as `store status`) and `clear`
    /// (the call that clears it). The refusal's `detail` is `{entity: {kind,
    /// name}, phase, op}` (#536 §10).
    pub fn blocks(
        &self,
        kind: &str,
        entity: &str,
        show: &str,
        clear: &str,
    ) -> Result<(), SdkRefusal> {
        let (name, id, phase, pid) = (self.name(), self.id(), self.phase(), self.pid());
        let shown = match kind {
            "store" => "Store",
            "sandbox" => "Sandbox",
            other => other,
        };
        let mut refusal = match self.standing() {
            Standing::Stale => return Ok(()),
            Standing::Running => refused(
                "Busy",
                format!(
                    "{shown} {entity} is busy: {name} ({id}, phase {phase}) runs in process {pid}"
                ),
            ),
            Standing::Unresolved => refused(
                "Unresolved",
                format!(
                    "{shown} {entity} has an unresolved {name} ({id}, phase {phase}): its outcome \
                     is not known; see `layerfs {show} {entity}`, then clear it with `layerfs \
                     {clear} {entity}`"
                ),
            ),
        };
        refusal.detail = SdkValue::map([
            (
                "entity",
                SdkValue::map([
                    ("kind", SdkValue::text(kind)),
                    ("name", SdkValue::text(entity)),
                ]),
            ),
            ("phase", SdkValue::text(phase)),
            ("op", SdkValue::text(id)),
        ]);
        Err(refusal)
    }

    /// The intent as `machine.status`, `store.status` and `sandbox.get`
    /// show it (H6): `{id, name, phase, started, uncertain}`.
    pub fn to_sdk(&self) -> SdkValue {
        let copied = |name: &str| self.fields.get(name).cloned().unwrap_or_default();
        SdkValue::map([
            ("id", copied("id")),
            ("name", copied("name")),
            ("phase", copied("phase")),
            ("started", copied("started")),
            ("uncertain", SdkValue::Bool(self.uncertain())),
        ])
    }

    /// The intent as its record holds it.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }

    /// Appends the uncertain outcome to `log/uncertain.jsonl` for people:
    /// `entity` names its kind (`store`, `machine`, `sandbox`) and `name`
    /// the entity. A log that cannot be written changes no outcome.
    pub fn log_uncertain(&self, home: &Path, entity: &str, name: &str, message: &str) {
        let _ = home::log_line(
            home,
            "uncertain.jsonl",
            &SdkValue::map([
                ("at", SdkValue::Int(home::now_ms())),
                ("op", SdkValue::text(self.id())),
                ("call", SdkValue::text(self.name())),
                (entity, SdkValue::text(name)),
                ("phase", SdkValue::text(self.phase())),
                ("message", SdkValue::text(message)),
            ]),
        );
    }
}

/// This process as an intent's `owner`.
fn owner() -> SdkValue {
    let fields = process::identity().into_iter();
    SdkValue::Map(
        fields
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

/// A fresh `op-<16 hex>`.
fn mint() -> Result<String, SdkRefusal> {
    let bytes = crate::helpers::random()?;
    let id = u64::from_be_bytes(bytes[..8].try_into().expect("eight bytes"));
    Ok(format!("op-{id:016x}"))
}
