//! The operational brakes and key of automatic reclamation: a separate
//! concern from the B5 Sandbox state transitions, so the lifecycle module
//! keeps only the transitions and their decisions.
use layerfs_bridge::control::SdkValue;

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
#[must_use]
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
