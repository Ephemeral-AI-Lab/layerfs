//! The GC admission record vocabulary: intents, retained done rows and the
//! results reconciliation reports. The same bounded, portable-data rule as
//! [`super::records`] applies.

use super::authority::PlaceAuthority;

/// One kind of operation admitted through the Store's `gc_intent` table (§5.1).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GcIntentKind {
    /// A mounted or mounting Workspace.
    Mount,
    /// A project Init.
    Init,
    /// A Branch merge.
    Merge,
    /// A soft removal (`branch.remove`, `project.delete`, `project.remove_file`).
    Removal,
    /// A Store compaction.
    Compaction,
}

impl GcIntentKind {
    /// The `gc_intent.kind` text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mount => "mount",
            Self::Init => "init",
            Self::Merge => "merge",
            Self::Removal => "removal",
            Self::Compaction => "compaction",
        }
    }

    /// True when the operation shares its scopes instead of excluding them.
    #[must_use]
    pub const fn shared(self) -> bool {
        matches!(self, Self::Mount | Self::Init | Self::Merge)
    }
}

/// The scope an admission intent is taken at.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GcScopeKind {
    /// The whole Store (compaction).
    Store,
    /// One project (Init, merge, `project.delete`, `remove_file`).
    Project,
    /// One Branch (mount, `branch.remove`).
    Branch,
}

impl GcScopeKind {
    /// The `gc_intent.scope_kind` text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Store => "store",
            Self::Project => "project",
            Self::Branch => "branch",
        }
    }
}

/// One durable admission row (§5.1), or the operation row a caller writes
/// before touching a Store. Ids are typed per kind: 32 bytes for the Store
/// catalog, 17 for a project or Branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GcIntent {
    /// 16-byte operation identity (`gc_intent.op_id`).
    pub op_id: [u8; 16],
    /// Operation kind.
    pub kind: GcIntentKind,
    /// Scope the intent is recorded at.
    pub scope_kind: GcScopeKind,
    /// Typed scope identity.
    pub scope_id: Vec<u8>,
    /// The owning project identity, when the scope is a Branch. It is not a
    /// stored column: it lets the PostgreSQL key derivation name the project
    /// scope of a Branch intent.
    pub project_id: Option<Vec<u8>>,
    /// Mounting Workspace identity, 32 bytes.
    pub workspace: Option<[u8; 32]>,
    /// Sandbox name, when the caller knows one.
    pub sandbox: Option<String>,
    /// Machine name, when the caller knows one.
    pub machine: Option<String>,
    /// Project name, when the caller knows it.
    pub project: Option<String>,
    /// Branch name, when the caller knows it.
    pub branch: Option<String>,
    /// Owning runtime token.
    pub owner_runtime: Option<String>,
    /// Owning boot token; `UNIQUE(scope_kind, scope_id, owner_boot)`.
    pub owner_boot: Option<String>,
    /// Operation phase, for a holder report.
    pub phase: Option<String>,
    /// The machine or worker generation that placed the intent, when known.
    pub machine_incarnation: Option<String>,
    /// The Sandbox generation that placed it, when known.
    pub sandbox_generation: Option<String>,
    /// True when the owner is uncertain and only `Unresolved` may report it.
    pub uncertain: bool,
}

impl GcIntent {
    /// A shared (`mount`, `init`, `merge`) intent at one scope.
    #[must_use]
    pub fn shared(
        op_id: [u8; 16],
        kind: GcIntentKind,
        scope_kind: GcScopeKind,
        scope_id: impl Into<Vec<u8>>,
    ) -> Self {
        Self::new(op_id, kind, scope_kind, scope_id)
    }

    /// An exclusive (`removal`, `compaction`) intent at one scope.
    #[must_use]
    pub fn operation(
        op_id: [u8; 16],
        kind: GcIntentKind,
        scope_kind: GcScopeKind,
        scope_id: impl Into<Vec<u8>>,
    ) -> Self {
        Self::new(op_id, kind, scope_kind, scope_id)
    }

    fn new(
        op_id: [u8; 16],
        kind: GcIntentKind,
        scope_kind: GcScopeKind,
        scope_id: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            op_id,
            kind,
            scope_kind,
            scope_id: scope_id.into(),
            project_id: None,
            workspace: None,
            sandbox: None,
            machine: None,
            project: None,
            branch: None,
            owner_runtime: None,
            owner_boot: None,
            phase: None,
            machine_incarnation: None,
            sandbox_generation: None,
            uncertain: false,
        }
    }

    /// Records the owning project identity of a Branch-scoped intent.
    #[must_use]
    pub fn in_project(mut self, project: impl Into<Vec<u8>>) -> Self {
        self.project_id = Some(project.into());
        self
    }

    /// Fills the project and Branch names a holder report shows.
    #[must_use]
    pub fn named(mut self, project: Option<&str>, branch: Option<&str>) -> Self {
        self.project = project.map(str::to_owned);
        self.branch = branch.map(str::to_owned);
        self
    }

    /// Fills the Workspace, Sandbox and machine a holder report shows.
    #[must_use]
    pub fn placed(
        mut self,
        workspace: Option<[u8; 32]>,
        sandbox: Option<&str>,
        machine: Option<&str>,
    ) -> Self {
        self.workspace = workspace;
        self.sandbox = sandbox.map(str::to_owned);
        self.machine = machine.map(str::to_owned);
        self
    }

    /// Fills the owner tokens.
    #[must_use]
    pub fn owned(mut self, runtime: Option<&str>, boot: Option<&str>) -> Self {
        self.owner_runtime = runtime.map(str::to_owned);
        self.owner_boot = boot.map(str::to_owned);
        self
    }

    /// Records the phase a holder report shows.
    #[must_use]
    pub fn with_phase(mut self, phase: Option<&str>) -> Self {
        self.phase = phase.map(str::to_owned);
        self
    }

    /// Records the place authority the intent carries.
    #[must_use]
    pub fn authorized(mut self, authority: PlaceAuthority) -> Self {
        self.machine_incarnation = authority.machine_incarnation;
        self.sandbox_generation = authority.sandbox_generation;
        self
    }

    /// The place authority this intent carries, for the Store-side admission
    /// that classifies it.
    #[must_use]
    pub fn authority(&self) -> PlaceAuthority {
        PlaceAuthority {
            machine_incarnation: self.machine_incarnation.clone(),
            sandbox_generation: self.sandbox_generation.clone(),
        }
    }

    /// Marks the owner uncertain: only `Unresolved` may report it.
    #[must_use]
    pub fn uncertain(mut self) -> Self {
        self.uncertain = true;
        self
    }
}

/// One retained `done` admission row, the operator audit of a settle or
/// release. The owner columns are cleared when the row is retained, so it
/// never blocks a later mount; admission queries filter `state <> 'done'`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GcIntentDone {
    /// 16-byte operation identity.
    pub op_id: [u8; 16],
    /// Operation kind.
    pub kind: GcIntentKind,
    /// Scope the operation was admitted at.
    pub scope_kind: GcScopeKind,
    /// Typed scope identity.
    pub scope_id: Vec<u8>,
    /// Project name, when recorded.
    pub project: Option<String>,
    /// Branch name, when recorded.
    pub branch: Option<String>,
    /// Machine name, when recorded.
    pub machine: Option<String>,
    /// Sandbox name, when recorded.
    pub sandbox: Option<String>,
    /// Operation phase the action left.
    pub phase: Option<String>,
    /// The operator action, operator and operation the release recorded.
    pub evidence: Option<String>,
    /// When the row was retained, in Unix milliseconds.
    pub updated_ms: i64,
}

/// One bounded page of the retained `done` audit, oldest first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GcAuditPage {
    /// The retained rows on this page.
    pub items: Vec<GcIntentDone>,
    /// The zero-based page number that was read.
    pub page: u32,
    /// The page size that was read (the caller's, clamped to the maximum).
    pub limit: u32,
    /// True when another page may follow this one.
    pub has_more: bool,
}

/// Where one Branch's audit page ends: the last row's `(updated_ms, op_id)`.
/// The next page reads the rows strictly after it. A release between two
/// reads deletes only older rows and stamps its own row after every row of
/// its Branch identity, so paging never repeats a row and never skips one of
/// that identity. The audit selects by project and Branch name: after a
/// Branch (or its project) is removed and created again under the same
/// names, the new Branch's rows are stamped from its own rows only and can
/// sort below the earlier Branch's, so a cursor already past them misses
/// them; a read from the first page returns every retained row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GcAuditAfter {
    /// The last row's retained time, in Unix milliseconds.
    pub updated_ms: i64,
    /// The last row's operation identity.
    pub op_id: [u8; 16],
}

/// One bounded keyset page of one Branch's retained `done` rows, oldest
/// first by `(updated_ms, op_id)`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GcBranchAuditPage {
    /// The retained rows on this page.
    pub items: Vec<GcIntentDone>,
    /// The page size that was read (the caller's, clamped to the maximum).
    pub limit: u32,
    /// Where the next page starts, when a row follows this page.
    pub next: Option<GcAuditAfter>,
}

/// The largest page one audit read returns, whatever the caller asks for.
pub const MAXIMUM_AUDIT_PAGE: u32 = 1000;

/// The outcome of the Store side of `project.delete`: a marker already there,
/// a marker written now, or neither a live stack nor a marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GcDeletion {
    /// The marker was already recorded; its `deleted_ms` is reused.
    Retired {
        /// The `deleted_ms` the marker holds.
        deleted_ms: i64,
    },
    /// A live stack was marked with the caller's `deleted_ms`.
    Marked {
        /// The `deleted_ms` written.
        deleted_ms: i64,
    },
    /// Neither a live stack nor a marker: the caller removes its stale entry.
    Absent,
}

impl GcDeletion {
    /// The `deleted_ms` a `project.delete` result reports.
    #[must_use]
    pub fn deleted_ms(self, requested: i64) -> i64 {
        match self {
            Self::Retired { deleted_ms } | Self::Marked { deleted_ms } => deleted_ms,
            Self::Absent => requested,
        }
    }
}

/// What one worker-start reconcile did at one Store: the machine authority it
/// recorded, the intents it settled, and the owners it kept.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GcReconcile {
    /// The machine whose authority was recorded.
    pub machine: String,
    /// The incarnation the Store now attributes to it.
    pub incarnation: String,
    /// The operation ids of the intents settled, in the order they were found.
    pub settled: Vec<[u8; 16]>,
    /// Intents kept because their place is live or their owner is uncertain.
    pub held: u64,
    /// The kept intents whose owner is uncertain (a subset of `held`).
    pub uncertain: u64,
}
