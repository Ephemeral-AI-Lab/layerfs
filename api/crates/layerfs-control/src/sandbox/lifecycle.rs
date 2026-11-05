//! The B5 Sandbox lifecycle (#548): a generation CAS into `Draining`, an
//! exec lease with a deadline, and the `Closed` / `Lost` outcomes. The
//! record is the authority and these functions are pure over its fields,
//! so every transition is testable without a runtime.
//!
//! The protocol is:
//!
//! ```text
//! Ready --CAS(generation)--> Draining --no live lease--> Closed
//!                                   \--lease expiry--> Lost
//!                                   \--unknown/panic--> Unresolved
//! ```
//!
//! A live lease blocks a close (`LeaseActive`); it is never a reason to
//! kill. An expired lease moves the record to `Lost`, which `sandbox get`
//! shows; a later close reclaims the record without touching a runtime.
use crate::common::home;
use crate::common::op::Standing;
use crate::common::outcome::refused;
use layerfs_bridge::control::{SdkRefusal, SdkValue};

/// The record field holding the Sandbox's generation.
pub const GENERATION: &str = "generation";
/// The record field holding the active exec leases.
pub const LEASES: &str = "exec_leases";
/// The record field holding the last activity from the monotonic clock.
pub const LAST_ACTIVITY: &str = "last_activity_monotonic";
/// The record field marking a Sandbox `Lost`.
pub const LOST: &str = "lost";
/// The record field recording why a close happened.
pub const CLOSE_REASON: &str = "close_reason";
/// The close reason an explicit close records before it stops anything:
/// `sandbox get` shows it if the closing process dies mid-drain.
pub const OPERATOR_CLOSE: &str = "an operator asked to close it";

/// How long an exec lease lasts without a heartbeat: fifteen minutes.
pub const EXEC_LEASE_MS: i64 = 15 * 60 * 1000;

/// One active exec lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecLease {
    /// The lease's identity, `lease-<16 hex>`.
    pub id: String,
    /// When it expires, in Unix milliseconds.
    pub deadline: i64,
}

/// A record's generation; `1` for a record written before generations.
pub fn generation_of(record: &SdkValue) -> u64 {
    record
        .get_int(GENERATION)
        .and_then(|generation| u64::try_from(generation).ok())
        .filter(|generation| *generation > 0)
        .unwrap_or(1)
}

/// The exec leases `record` holds.
pub fn leases_of(record: &SdkValue) -> Vec<ExecLease> {
    let Some(SdkValue::List(items)) = record.get(LEASES) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            Some(ExecLease {
                id: item.get_text("id")?.to_owned(),
                deadline: item.get_int("deadline")?,
            })
        })
        .collect()
}

/// `record` with lease `id` held until `deadline`.
pub fn with_lease(record: SdkValue, id: &str, deadline: i64) -> SdkValue {
    let mut leases = leases_of(&record);
    leases.retain(|lease| lease.id != id);
    leases.push(ExecLease {
        id: id.to_owned(),
        deadline,
    });
    let list = SdkValue::List(
        leases
            .into_iter()
            .map(|lease| {
                SdkValue::map([
                    ("id", SdkValue::text(&lease.id)),
                    ("deadline", SdkValue::Int(lease.deadline)),
                ])
            })
            .collect(),
    );
    home::with_field(record, LEASES, list)
}

/// `record` without lease `id`.
pub fn without_lease(record: SdkValue, id: &str) -> SdkValue {
    let leases: Vec<ExecLease> = leases_of(&record)
        .into_iter()
        .filter(|lease| lease.id != id)
        .collect();
    if leases.is_empty() {
        return without_field(record, LEASES);
    }
    let list = SdkValue::List(
        leases
            .into_iter()
            .map(|lease| {
                SdkValue::map([
                    ("id", SdkValue::text(&lease.id)),
                    ("deadline", SdkValue::Int(lease.deadline)),
                ])
            })
            .collect(),
    );
    home::with_field(record, LEASES, list)
}

/// `record` with every exec lease removed.
pub fn without_leases(record: SdkValue) -> SdkValue {
    without_field(record, LEASES)
}

/// `record` with its last activity set from the monotonic clock.
pub fn with_activity(record: SdkValue, monotonic_ms: i64) -> SdkValue {
    home::with_field(record, LAST_ACTIVITY, SdkValue::Int(monotonic_ms))
}

/// `record` marked `Lost`: no intent, no lease, `reason` recorded, and the
/// monotonic clock as the evidence. `sandbox get` shows `state: lost`.
pub fn with_lost(record: SdkValue, reason: &str, monotonic_ms: i64) -> SdkValue {
    let record = without_field(record, crate::common::op::FIELD);
    let record = without_leases(record);
    let record = home::with_field(record, LOST, SdkValue::Bool(true));
    let record = home::with_field(record, CLOSE_REASON, SdkValue::text(reason));
    with_activity(record, monotonic_ms)
}

/// `record` without `field`.
pub fn without_field(record: SdkValue, field: &str) -> SdkValue {
    match record {
        SdkValue::Map(mut fields) => {
            fields.retain(|(name, _)| name != field);
            SdkValue::Map(fields)
        }
        other => other,
    }
}

/// Whether `record` is `Lost`.
pub fn is_lost(record: &SdkValue) -> bool {
    record.get_bool(LOST)
}

/// A Sandbox's lifecycle state (#548), derived from its record and intent
/// and never stored by layout 3. `#[non_exhaustive]`: a later state is added
/// without breaking a reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SandboxState {
    /// A create claimed the name and is still writing its record.
    Creating,
    /// A create is provisioning its runtime, Store or image.
    Provisioning,
    /// A create is waiting for its runtime's daemon to become ready.
    Starting,
    /// Started, with no live exec lease and no operation in flight.
    Ready,
    /// Started, with at least one unexpired exec lease.
    Busy,
    /// A close won the generation CAS and is draining the Sandbox.
    Draining,
    /// Stopped and reclaimed: no runtime is recorded, or the machine says
    /// no such Sandbox runs.
    Closed,
    /// Its runtime ended without a close: an expired lease or a provider
    /// that no longer has it.
    Lost,
    /// A create failed and its record was kept with the failure.
    Failed,
    /// An operation's outcome is not known: a gone owner left it uncertain.
    Unresolved,
}

impl SandboxState {
    /// Every state, in lifecycle order.
    pub const ALL: [SandboxState; 10] = [
        Self::Creating,
        Self::Provisioning,
        Self::Starting,
        Self::Ready,
        Self::Busy,
        Self::Draining,
        Self::Closed,
        Self::Lost,
        Self::Failed,
        Self::Unresolved,
    ];

    /// The state's name in `sandbox.get` and `sandbox.list`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Creating => "creating",
            Self::Provisioning => "provisioning",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Busy => "busy",
            Self::Draining => "draining",
            Self::Closed => "closed",
            Self::Lost => "lost",
            Self::Failed => "failed",
            Self::Unresolved => "unresolved",
        }
    }

    /// The state a record's explicit `state` field names, including the
    /// names the ten explicit states replaced (`running`, `gone`,
    /// `closing`, `unknown`).
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "creating" => Some(Self::Creating),
            "provisioning" => Some(Self::Provisioning),
            "starting" => Some(Self::Starting),
            "ready" | "running" => Some(Self::Ready),
            "busy" => Some(Self::Busy),
            "draining" | "closing" => Some(Self::Draining),
            "closed" | "gone" => Some(Self::Closed),
            "lost" => Some(Self::Lost),
            "failed" => Some(Self::Failed),
            "unresolved" | "unknown" => Some(Self::Unresolved),
            _ => None,
        }
    }
}

impl std::fmt::Display for SandboxState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The record, `op`, lease and observation facts a Sandbox's state is
/// derived from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SandboxFacts<'a> {
    /// A `state` a record written before layout 3 still carries.
    pub recorded: Option<SandboxState>,
    /// The record is marked `Lost`.
    pub lost: bool,
    /// A close won the generation CAS and its process ended.
    pub close_crashed: bool,
    /// The record's intent: standing, operation name and phase.
    pub op: Option<(Standing, &'a str, &'a str)>,
    /// The machine answered that no such Sandbox runs.
    pub gone: bool,
    /// The record names a runtime id.
    pub started: bool,
    /// How many exec leases have not expired.
    pub live_leases: usize,
}

/// The one mapping from a record's facts to its [`SandboxState`]. It is the
/// only place the facts decide a state; adapters serialize the result.
pub fn sandbox_state(facts: SandboxFacts<'_>) -> SandboxState {
    use SandboxState as S;
    // Lost outranks every recorded and derived state: an expired lease or
    // a gone provider is evidence the runtime ended.
    if facts.lost {
        return S::Lost;
    }
    // A close that won its generation CAS and then died left an outcome no
    // one can know.
    if facts.close_crashed {
        return S::Unresolved;
    }
    // A live or uncertain intent describes what is happening now, before
    // any state an older record stored.
    match facts.op {
        // A live close, whichever phase it wrote, is draining.
        Some((Standing::Running, "sandbox.close", _)) => return S::Draining,
        Some((Standing::Running, "sandbox.create", phase)) => {
            return match phase {
                "claim" => S::Creating,
                "provision" | "provisioning" => S::Provisioning,
                // "start", "starting" and any later phase
                _ => S::Starting,
            };
        }
        // Any other live intent is still creating its Sandbox, as before.
        Some((Standing::Running, _, _)) => return S::Creating,
        // A gone owner's uncertain intent has an unknown outcome; its
        // certain one holds nothing and falls through.
        Some((Standing::Unresolved, _, _)) => return S::Unresolved,
        _ => {}
    }
    // A record written before layout 3 may still carry its explicit state
    // (`failed` is what keeps a recorded failure). Every layout 3 record
    // has none, so the runtime facts below decide, as before.
    if let Some(recorded) = facts.recorded {
        return recorded;
    }
    if facts.gone || !facts.started {
        S::Closed
    } else if facts.live_leases > 0 {
        S::Busy
    } else {
        S::Ready
    }
}

/// The explicit `state` a record written before layout 3 still carries,
/// read as the enum it names; `None` when absent or unknown.
pub fn recorded_state(record: &SdkValue) -> Option<SandboxState> {
    record.get_text("state").and_then(SandboxState::from_name)
}

/// The close protocol's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseState {
    /// `Ready` or `Busy`: a close may request `Draining`.
    Ready,
    /// The close won the generation CAS and is checking its lease.
    Draining,
    /// The close finished (or a replay found it finished).
    Closed,
    /// The Sandbox is gone or its lease expired; nothing is killed.
    Lost,
    /// A close ended with an unknown outcome.
    Unresolved,
}

/// One event the close protocol handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseEvent {
    /// A close named `expected`; the record is at `current`.
    Request {
        /// The generation the caller named.
        expected: u64,
        /// The generation the record carries.
        current: u64,
        /// How many exec leases have not expired.
        live_leases: usize,
        /// How many exec leases have expired.
        expired_leases: usize,
    },
    /// Stop and delete answered.
    Stopped,
    /// The provider no longer has the Sandbox.
    ProviderGone,
    /// The stop's outcome is not known.
    Unknown,
    /// A certain failure left the Sandbox as it was.
    Failed,
    /// The closing process panicked or exited.
    Panic,
    /// A replay of a close that already reached `generation`.
    Replay {
        /// The generation the earlier close reached.
        generation: u64,
    },
}

/// Why the close protocol refused a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseRefusal {
    /// The caller named a generation that is not the record's.
    StaleGeneration,
    /// An exec lease is live; a close must not kill it.
    LeaseActive,
    /// The event does not belong to the state.
    Illegal,
}

/// What the close protocol says to do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseDecision {
    /// Win the generation CAS and enter `Draining`.
    Drain,
    /// Abort `Draining`; the Sandbox stays as it was.
    Ready,
    /// The Sandbox is closed.
    Closed,
    /// The Sandbox is `Lost`; never a blind kill.
    Lost,
    /// The outcome is unknown; the record keeps an uncertain intent.
    Unresolved,
    /// The request is refused with this reason.
    Refuse(CloseRefusal),
}

/// The close protocol's transition function. Every state and event the
/// protocol admits is handled here, so a test can cover each one.
pub fn close_transition(state: CloseState, event: CloseEvent) -> CloseDecision {
    use CloseDecision as D;
    use CloseEvent as E;
    match (state, event) {
        (
            CloseState::Ready,
            E::Request {
                expected, current, ..
            },
        ) if expected != current => D::Refuse(CloseRefusal::StaleGeneration),
        (CloseState::Ready, E::Request { live_leases, .. }) if live_leases > 0 => {
            D::Refuse(CloseRefusal::LeaseActive)
        }
        (CloseState::Ready, E::Request { expired_leases, .. }) if expired_leases > 0 => D::Lost,
        (CloseState::Ready, E::Request { .. }) => D::Drain,
        (CloseState::Draining, E::Stopped) => D::Closed,
        (CloseState::Draining, E::ProviderGone) => D::Lost,
        (CloseState::Draining, E::Unknown | E::Panic) => D::Unresolved,
        (CloseState::Draining, E::Failed) => D::Ready,
        (CloseState::Closed, E::Replay { .. }) => D::Closed,
        (CloseState::Closed, E::Request { .. }) => D::Closed,
        (CloseState::Lost, E::Replay { .. }) => D::Lost,
        (CloseState::Lost, E::Request { .. }) => D::Lost,
        (CloseState::Unresolved, E::Replay { .. }) => D::Unresolved,
        (CloseState::Unresolved, E::Request { .. }) => D::Unresolved,
        _ => D::Refuse(CloseRefusal::Illegal),
    }
}

/// The refusal of a close that named a stale generation.
pub fn stale_generation(name: &str, expected: u64, current: u64) -> SdkRefusal {
    let mut refusal = refused(
        "StaleGeneration",
        format!(
            "Sandbox {name}'s close named generation {expected}, but its record is at generation \
             {current}; nothing was changed"
        ),
    );
    refusal.detail = SdkValue::map([
        ("sandbox", SdkValue::text(name)),
        (
            "expected",
            SdkValue::Int(i64::try_from(expected).unwrap_or(i64::MAX)),
        ),
        (
            "current",
            SdkValue::Int(i64::try_from(current).unwrap_or(i64::MAX)),
        ),
    ]);
    refusal
}

/// The refusal of a close while an exec lease is live.
pub fn lease_active(name: &str, leases: &[ExecLease]) -> SdkRefusal {
    let ids: Vec<SdkValue> = leases
        .iter()
        .map(|lease| SdkValue::text(&lease.id))
        .collect();
    let named: Vec<&str> = leases.iter().map(|lease| lease.id.as_str()).collect();
    let deadline = leases.iter().map(|lease| lease.deadline).max().unwrap_or(0);
    let mut refusal = refused(
        "LeaseActive",
        format!(
            "Sandbox {name} has {} live exec lease(s) ({}): a close waits for them and never \
             kills a running command",
            leases.len(),
            named.join(", ")
        ),
    );
    refusal.detail = SdkValue::map([
        ("sandbox", SdkValue::text(name)),
        ("leases", SdkValue::List(ids)),
        ("lease_deadline", SdkValue::Int(deadline)),
    ]);
    refusal
}

/// The refusal of a close that found an expired lease: the Sandbox is
/// marked `Lost`, nothing is killed.
pub fn lost(name: &str, reason: &str) -> SdkRefusal {
    let mut refusal = refused(
        "Lost",
        format!(
            "Sandbox {name} is Lost ({reason}); it was not killed. A later close reclaims the \
             record when no lease is live"
        ),
    );
    refusal.detail = SdkValue::map([
        ("sandbox", SdkValue::text(name)),
        ("state", SdkValue::text("lost")),
        ("reason", SdkValue::text(reason)),
    ]);
    refusal
}

/// The operational brakes of automatic reclamation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReclaimPolicy {
    /// `reclaim.enabled`: the master switch.
    pub enabled: bool,
    /// `--dry-run`: decide and record, delete nothing.
    pub dry_run: bool,
    /// The freeze / kill switch.
    pub freeze: bool,
}

impl ReclaimPolicy {
    /// The policy from `LAYERFS_RECLAIM_ENABLED`, `LAYERFS_RECLAIM_DRY_RUN`
    /// and `LAYERFS_RECLAIM_FREEZE`; enabled unless set false.
    pub fn from_env() -> Self {
        Self {
            enabled: flag("LAYERFS_RECLAIM_ENABLED", true),
            dry_run: flag("LAYERFS_RECLAIM_DRY_RUN", false),
            freeze: flag("LAYERFS_RECLAIM_FREEZE", false),
        }
    }

    /// A policy built from its three brakes.
    pub fn new(enabled: bool, dry_run: bool, freeze: bool) -> Self {
        Self {
            enabled,
            dry_run,
            freeze,
        }
    }

    /// Whether the destructive path may run.
    pub fn allows(&self) -> bool {
        self.enabled && !self.dry_run && !self.freeze
    }

    /// What the destructive path may do. The disabled and frozen brakes
    /// come first: a dry run never overrides an operator's stop.
    pub fn decide(&self) -> ReclaimDecision {
        if !self.enabled {
            return ReclaimDecision::Blocked("reclaim is disabled (LAYERFS_RECLAIM_ENABLED)");
        }
        if self.freeze {
            return ReclaimDecision::Blocked("reclaim is frozen (LAYERFS_RECLAIM_FREEZE)");
        }
        if self.dry_run {
            return ReclaimDecision::DryRun;
        }
        ReclaimDecision::Reclaim
    }

    /// Why the destructive path is stopped, or `None` when it may run.
    pub fn brake(&self) -> Option<&'static str> {
        self.decide().brake()
    }
}

/// What automatic reclamation may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReclaimDecision {
    /// Delete the stopped runtime and its folder.
    Reclaim,
    /// Decide and record the outcome, delete nothing.
    DryRun,
    /// Keep the record, mark it `Lost`, and record why.
    Blocked(&'static str),
}

impl ReclaimDecision {
    /// Why the destructive path is stopped, or `None` when it may run.
    pub fn brake(self) -> Option<&'static str> {
        match self {
            Self::Reclaim => None,
            Self::DryRun => Some("reclaim is a dry run (LAYERFS_RECLAIM_DRY_RUN)"),
            Self::Blocked(brake) => Some(brake),
        }
    }
}

/// The backend a Sandbox runs on, as reclamation keys it: `native`,
/// `namespaces` or `docker`. The isolation class (`process`, `container`,
/// `vm`, `remote`) is not part of it and never changes the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReclaimBackend {
    /// A native host-process daemon (macOS).
    Native,
    /// A Linux namespaces Sandbox.
    Namespaces,
    /// A Docker Engine container.
    Docker,
}

impl ReclaimBackend {
    /// The backend's name in records and read models.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Namespaces => "namespaces",
            Self::Docker => "docker",
        }
    }
}

/// The backend a record runs on, from its `backend` field and, for older
/// records, its `runtime`. A Docker record has no `runtime` (Docker is its
/// backend). The isolation class is never consulted.
pub fn reclaim_backend(record: &SdkValue) -> ReclaimBackend {
    match record
        .get_text("backend")
        .or_else(|| record.get_text("runtime"))
    {
        Some("namespaces") => ReclaimBackend::Namespaces,
        Some("native" | "local") => ReclaimBackend::Native,
        _ => ReclaimBackend::Docker,
    }
}

/// What automatic reclamation is keyed by: the machine that owns the
/// Sandbox and its backend. The isolation class is never part of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReclaimKey {
    /// The machine that runs the Sandbox.
    pub machine: String,
    /// Its backend.
    pub backend: ReclaimBackend,
}

impl ReclaimKey {
    /// The key `record` names: its `machine` and [`reclaim_backend`].
    pub fn of(record: &SdkValue) -> Self {
        Self {
            machine: record.get_text("machine").unwrap_or("native").to_owned(),
            backend: reclaim_backend(record),
        }
    }

    /// A key from its parts.
    pub fn new(machine: impl Into<String>, backend: ReclaimBackend) -> Self {
        Self {
            machine: machine.into(),
            backend,
        }
    }

    /// The key as a read model: `{ machine, backend }`.
    pub fn to_sdk(&self) -> SdkValue {
        SdkValue::map([
            ("machine", SdkValue::text(&self.machine)),
            ("backend", SdkValue::text(self.backend.as_str())),
        ])
    }
}

fn flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(value) => !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off" | ""
        ),
        Err(_) => default,
    }
}
