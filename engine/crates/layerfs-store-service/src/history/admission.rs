//! Store-service side of GC admission: operation ids, intent builders and the
//! short-lived guard a shared Init/merge holds.
use crate::history::lookup;
use layerfs_history::records::{
    GcAuditAfter, GcAuditPage, GcBranchAuditPage, GcIntent, GcIntentDone, GcIntentKind,
    GcReconcile, GcScopeKind,
};
use layerfs_history::{
    BranchRecord, HistoryCatalog, HistoryResult, LayerStackId, LayerStackRecord, PlaceAuthority,
    PlaceView,
};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_OP: AtomicU64 = AtomicU64::new(0);

/// Installs the bounded place view the control layer built from its home for
/// the duration of one Store call. The engine reads only this value; it never
/// opens a home record. A call that supplies \`None\` keeps the admission the
/// caller has always had.
pub use layerfs_persistence::{has_place_view, with_place_view};

/// The bounded place view as the worker frame carries it. One entry per
/// machine and Sandbox the control home could see; a name it could not read is
/// absent, and absence is no proof.
#[must_use]
pub fn encode_place_view(view: &PlaceView) -> SdkValue {
    let machines = view
        .machines()
        .iter()
        .map(|(machine, incarnation)| {
            SdkValue::map([
                ("machine", SdkValue::text(machine)),
                ("incarnation", SdkValue::text(incarnation)),
            ])
        })
        .collect();
    let sandboxes = view
        .sandboxes()
        .iter()
        .map(|((machine, sandbox), place)| {
            SdkValue::map([
                ("machine", SdkValue::text(machine)),
                ("sandbox", SdkValue::text(sandbox)),
                (
                    "generation",
                    SdkValue::opt_text(place.generation.as_deref()),
                ),
                ("retired", SdkValue::opt_text(place.retired.as_deref())),
            ])
        })
        .collect();
    SdkValue::map([
        ("machines", SdkValue::List(machines)),
        ("sandboxes", SdkValue::List(sandboxes)),
    ])
}

/// The bounded place view a worker frame carries, when it carries one. An
/// absent or empty value is \`None\`: the admission falls back to the
/// caller's own recorded place, never to a guess.
#[must_use]
pub fn decode_place_view(value: Option<&SdkValue>) -> Option<PlaceView> {
    let value = value?;
    let mut view = PlaceView::new();
    if let Some(SdkValue::List(items)) = value.get("machines") {
        for item in items {
            if let (Some(machine), Some(incarnation)) =
                (item.get_text("machine"), item.get_text("incarnation"))
            {
                view.set_machine(machine, incarnation);
            }
        }
    }
    if let Some(SdkValue::List(items)) = value.get("sandboxes") {
        for item in items {
            let (Some(machine), Some(sandbox)) =
                (item.get_text("machine"), item.get_text("sandbox"))
            else {
                continue;
            };
            if let Some(generation) = item.get_text("generation") {
                view.set_sandbox_generation(machine, sandbox, generation);
            }
            if let Some(retired) = item.get_text("retired") {
                view.set_sandbox_retired(machine, sandbox, retired);
            }
        }
    }
    if view.is_empty() {
        None
    } else {
        Some(view)
    }
}

/// One fresh 16-byte operation identity.
pub fn op_id() -> [u8; 16] {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let mut seed = Vec::with_capacity(32);
    seed.extend_from_slice(&std::process::id().to_le_bytes());
    seed.extend_from_slice(&now.to_le_bytes());
    seed.extend_from_slice(&NEXT_OP.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    let identity = layerfs_content::ObjectId::for_bytes(&seed);
    let mut body = [0u8; 16];
    body.copy_from_slice(&identity.as_bytes()[..16]);
    body
}

/// A `branch.remove` intent at Branch scope, in its marker phase. A
/// concurrent remove that meets this live intent reports `Busy` with
/// `phase: "retire"` (spec §4.4, contract §2).
pub fn branch_removal(stack: &LayerStackRecord, branch: &BranchRecord) -> GcIntent {
    let op = op_id();
    owned(
        GcIntent::operation(
            op,
            GcIntentKind::Removal,
            GcScopeKind::Branch,
            branch.id.as_slice(),
        )
        .in_project(stack.id.as_slice())
        .named(Some(stack.name.as_str()), Some(branch.name.as_str())),
        op,
    )
    .with_phase(Some("retire"))
}

/// A `project.delete` or `project.remove_file` intent at project scope.
pub fn project_removal(stack: &LayerStackRecord) -> GcIntent {
    let op = op_id();
    owned(
        GcIntent::operation(
            op,
            GcIntentKind::Removal,
            GcScopeKind::Project,
            stack.id.as_slice(),
        )
        .named(Some(stack.name.as_str()), None),
        op,
    )
}

/// A `project.delete` intent when only the stack id is known.
pub fn project_removal_from_id(stack: LayerStackId) -> GcIntent {
    project_removal_at(stack, op_id(), None)
}

/// A `project.delete` intent under an operation identity the caller already
/// registered. The control layer admits the project scope before its sidecar
/// and marker and reuses the identity there, so one row is taken and released
/// once, never two competing rows.
///
/// `project` is the name the caller knows, when it knows one: the intent is a
/// stored row, and a later caller meeting it reports `Busy`/`Unresolved` with
/// the row's entity name, so an unnamed intent would name the hex LayerStack
/// id (spec §10). The worker fills it from the Stack record when the caller
/// sent none.
pub fn project_removal_at(stack: LayerStackId, op: [u8; 16], project: Option<&str>) -> GcIntent {
    owned(
        GcIntent::operation(
            op,
            GcIntentKind::Removal,
            GcScopeKind::Project,
            stack.as_slice(),
        )
        .named(project, None),
        op,
    )
}

/// A lowercase hex token, used to keep one process's owner tokens distinct.
fn hex16(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// This process's PID-namespace inode, when the platform names one. A pid is
/// only meaningful inside the namespace that wrote it.
fn pid_namespace() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let target = std::fs::read_link("/proc/self/ns/pid").ok()?;
        let text = target.to_str()?;
        text.strip_prefix("pid:[")?.strip_suffix(']')?.parse().ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// The owner evidence every intent records: this process (`pid` and its start
/// time, plus the PID namespace that makes the id meaningful) and this boot.
/// A later admission in the same namespace can prove the owner gone after a
/// `machine.stop` or SIGKILL. A call in another PID namespace stays
/// conservatively live: `/proc` may be one namespace's view and another
/// user's processes can be unreadable, so absence there never proves death;
/// the holder it reports names the Sandbox, machine and operation, and its
/// owner (or an operator with `gc_mark_uncertain` and `gc_release`) settles
/// it. A local SQLite Store is bounded further: every admission runs on the
/// machine that holds the file, so an intent from an earlier boot is provably
/// gone and is adopted. The boot token carries a unique suffix (the operation
/// or Workspace id), because the schema's
/// `UNIQUE(scope_kind, scope_id, owner_boot)` must still admit two intents of
/// one scope from one process.
#[must_use]
pub fn owner_tokens(suffix: &str) -> (String, String) {
    let pid = std::process::id();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        // An unreadable start is recorded as 0 and an unreadable boot as
        // `unknown`; admission reads either as missing evidence, never as
        // proof that the owner is gone (#776 G2).
        let start = layerfs_os::process::stat(pid)
            .map(|stat| stat.start)
            .unwrap_or(0);
        let boot = layerfs_os::process::boot()
            .map_or_else(|_| "unknown".to_owned(), |boot| format!("{boot:032x}"));
        let namespace = match pid_namespace() {
            Some(namespace) => format!("@ns:{namespace}"),
            None => String::new(),
        };
        (
            format!("pid:{pid}@{start}{namespace}"),
            format!("boot:{boot}:{suffix}"),
        )
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        (format!("pid:{pid}"), format!("boot:unknown:{suffix}"))
    }
}

/// The owner tokens of one operation identity, unique within its scope.
fn owned(mut intent: GcIntent, op: [u8; 16]) -> GcIntent {
    let (runtime, boot) = owner_tokens(&format!("op:{}", hex16(&op)));
    intent.owner_runtime = Some(runtime);
    intent.owner_boot = Some(boot);
    intent
}

/// A mount intent at Branch scope, naming its project identity for the
/// PostgreSQL key derivation and its project name for the operator audit (a
/// released row is found by project and Branch name). The Workspace is always
/// placed; the Sandbox and machine are placed only when the caller that
/// mounted knows them (a daemon mount does; a private Store-call Workspace
/// does not).
///
/// `authority` is the place evidence the caller read from its own home
/// records: a machine incarnation and Sandbox generation. A caller with none
/// (a private Store-call Workspace) passes [`PlaceAuthority::default`], and
/// the intent records only the owner tokens: admission then falls back to
/// this process's namespace and boot, never to a place it does not have. No
/// process-wide value is filled in: a place the caller did not state is not
/// recorded.
pub fn mount_intent(
    workspace: layerfs_history::WorkspaceId,
    snapshot: &layerfs_history::BranchSnapshot,
    project: Option<&str>,
    place: &layerfs_protocol::daemon::MountPlace,
    authority: PlaceAuthority,
) -> GcIntent {
    let body: [u8; 32] = workspace.as_slice().try_into().unwrap_or([0u8; 32]);
    let (runtime, boot) = owner_tokens(&format!("ws:{}", hex16(&body)));
    GcIntent::shared(
        op_id(),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        snapshot.branch.id.as_slice(),
    )
    .in_project(snapshot.branch.stack.as_slice())
    .named(project, Some(snapshot.branch.name.as_str()))
    .placed(
        Some(body),
        place.sandbox.as_deref(),
        place.machine.as_deref(),
    )
    .owned(Some(&runtime), Some(&boot))
    .authorized(authority)
}

/// Releases one intent. When the Store refuses the delete, the owner has gone
/// (a failed unmount or a lost bind): the row is marked uncertain instead of
/// staying `live`, so a later call reports `Unresolved` and the owning call
/// can settle it, rather than every GC call meeting `MountsActive` forever.
pub fn release_or_mark(
    catalog: &dyn HistoryCatalog,
    op_id: &[u8; 16],
) -> Result<(), layerfs_history::HistoryError> {
    match catalog.gc_release(op_id) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = catalog.gc_mark_uncertain(op_id);
            Err(error)
        }
    }
}

/// A short-lived shared intent (Init or merge), released on success or drop.
pub struct SharedIntent<'a> {
    catalog: &'a dyn HistoryCatalog,
    op_id: [u8; 16],
    scope_kind: GcScopeKind,
    entity: String,
    phase: String,
    admitted: bool,
    released: bool,
}

impl<'a> SharedIntent<'a> {
    /// Registers a shared intent at project scope.
    pub fn begin(
        catalog: &'a dyn HistoryCatalog,
        kind: GcIntentKind,
        stack: &LayerStackRecord,
        branch: Option<&str>,
    ) -> Result<Self, SdkRefusal> {
        let op = op_id();
        // The call knows the scope, project, phase and operation it held, so
        // a lost registration is `Unknown` naming them (spec §10 A6).
        let scope_kind = GcScopeKind::Project;
        let entity = stack.name.as_str().to_owned();
        let phase = kind.as_str().to_owned();
        let intent = GcIntent::shared(op, kind, scope_kind, stack.id.as_slice())
            .named(Some(stack.name.as_str()), branch)
            .with_phase(Some(&phase));
        let intent = owned(intent, op);
        let admitted = match catalog.gc_register(&intent) {
            Ok(()) => true,
            // Only a catalog with no admission at all proceeds unadmitted; a
            // real provider's `Unsupported` (a pooled connection) refuses.
            Err(error) if error.no_admission() => false,
            Err(error) => {
                return Err(lookup::history_in(
                    error,
                    scope_kind,
                    &entity,
                    Some(&phase),
                    Some(&op),
                ))
            }
        };
        Ok(Self {
            catalog,
            op_id: op,
            scope_kind,
            entity,
            phase,
            admitted,
            released: false,
        })
    }

    /// Releases the intent, reporting a failed delete.
    pub fn release(mut self) -> Result<(), SdkRefusal> {
        self.released = true;
        if !self.admitted {
            return Ok(());
        }
        match self.catalog.gc_release(&self.op_id) {
            Ok(()) | Err(layerfs_history::HistoryError::Unsupported(_)) => Ok(()),
            Err(error) => Err(lookup::history_in(
                error,
                self.scope_kind,
                &self.entity,
                Some(&self.phase),
                Some(&self.op_id),
            )),
        }
    }
}

impl Drop for SharedIntent<'_> {
    fn drop(&mut self) {
        if !self.released && self.admitted {
            // A release that is refused leaves the row uncertain, so a later
            // call reports `Unresolved` instead of a permanent `Busy`.
            let _ = release_or_mark(self.catalog, &self.op_id);
        }
    }
}

/// Settles the intents at `machine` the ordered `incarnation` proves dead,
/// never an uncertain one.
pub fn reconcile_machine(
    catalog: &dyn HistoryCatalog,
    machine: &str,
    incarnation: &str,
) -> HistoryResult<GcReconcile> {
    catalog.gc_reconcile_machine(machine, incarnation)
}

/// What admission would say of each of `intents` now (owner evidence, then
/// the place view installed on this thread), in order. Changes nothing.
pub fn liveness(
    catalog: &dyn HistoryCatalog,
    intents: &[GcIntent],
) -> HistoryResult<Vec<layerfs_history::Liveness>> {
    catalog.gc_liveness(intents)
}

/// One bounded page of retained `done` admission rows, for the operator
/// audit. This is the only read that sees a done row; admission and preview
/// keep filtering them out.
pub fn done_intents(
    catalog: &dyn HistoryCatalog,
    page: u32,
    limit: u32,
) -> HistoryResult<GcAuditPage> {
    catalog.gc_done_intents(page, limit)
}

/// One bounded keyset page of one Branch's retained `done` rows, for the
/// operator audit: the rows strictly after `after`, oldest first by
/// `(updated_ms, op_id)`, scoped to the Branch the call names.
pub fn done_intents_branch(
    catalog: &dyn HistoryCatalog,
    project: &str,
    branch: &str,
    after: Option<GcAuditAfter>,
    limit: u32,
) -> HistoryResult<GcBranchAuditPage> {
    catalog.gc_done_intents_branch(project, branch, after, limit)
}

/// The wire answer of one retained `done` row of the operator audit.
pub fn done_value(row: &GcIntentDone) -> SdkValue {
    SdkValue::map([
        ("op", SdkValue::text(hex16(&row.op_id))),
        ("kind", SdkValue::text(row.kind.as_str())),
        ("scope_kind", SdkValue::text(row.scope_kind.as_str())),
        ("scope_id", SdkValue::text(hex16(&row.scope_id))),
        ("project", SdkValue::opt_text(row.project.as_deref())),
        ("branch", SdkValue::opt_text(row.branch.as_deref())),
        ("machine", SdkValue::opt_text(row.machine.as_deref())),
        ("sandbox", SdkValue::opt_text(row.sandbox.as_deref())),
        ("phase", SdkValue::opt_text(row.phase.as_deref())),
        ("evidence", SdkValue::opt_text(row.evidence.as_deref())),
        ("updated_ms", SdkValue::Int(row.updated_ms)),
    ])
}

/// The wire answer of one reconcile: the authority recorded, the settled
/// operations and the owners it kept.
pub fn reconcile_value(reconcile: &GcReconcile) -> SdkValue {
    SdkValue::map([
        ("machine", SdkValue::text(&reconcile.machine)),
        ("incarnation", SdkValue::text(&reconcile.incarnation)),
        (
            "settled",
            SdkValue::List(
                reconcile
                    .settled
                    .iter()
                    .map(|op| SdkValue::text(hex16(op)))
                    .collect(),
            ),
        ),
        (
            "held",
            SdkValue::Int(i64::try_from(reconcile.held).unwrap_or(i64::MAX)),
        ),
        (
            "uncertain",
            SdkValue::Int(i64::try_from(reconcile.uncertain).unwrap_or(i64::MAX)),
        ),
    ])
}
