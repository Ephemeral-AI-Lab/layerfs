//! Store-service side of GC admission: operation ids, intent builders and the
//! short-lived guard a shared Init/merge holds.
use crate::history::lookup;
use layerfs_bridge::control::SdkRefusal;
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{BranchRecord, HistoryCatalog, LayerStackId, LayerStackRecord};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_OP: AtomicU64 = AtomicU64::new(0);

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
        let start = layerfs_bridge::process::stat(pid)
            .map(|stat| stat.start)
            .unwrap_or(0);
        let boot = layerfs_bridge::process::boot().unwrap_or(0);
        let namespace = match pid_namespace() {
            Some(namespace) => format!("@ns:{namespace}"),
            None => String::new(),
        };
        (
            format!("pid:{pid}@{start}{namespace}"),
            format!("boot:{boot:032x}:{suffix}"),
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
/// PostgreSQL key derivation. The Workspace is always placed; the Sandbox
/// and machine are placed only when the caller that mounted knows them
/// (a daemon mount does; a private Store-call Workspace does not). The owner
/// evidence is always recorded, so a gone mount can be told from a live one.
pub fn mount_intent(
    workspace: layerfs_history::WorkspaceId,
    snapshot: &layerfs_history::BranchSnapshot,
    place: &layerfs_bridge::control::MountPlace,
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
    .named(None, Some(snapshot.branch.name.as_str()))
    .placed(
        Some(body),
        place.sandbox.as_deref(),
        place.machine.as_deref(),
    )
    .owned(Some(&runtime), Some(&boot))
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
        let intent = GcIntent::shared(op, kind, GcScopeKind::Project, stack.id.as_slice())
            .named(Some(stack.name.as_str()), branch)
            .with_phase(Some(kind.as_str()));
        let intent = owned(intent, op);
        let admitted = match catalog.gc_register(&intent) {
            Ok(()) => true,
            Err(layerfs_history::HistoryError::Unsupported(_)) => false,
            Err(error) => return Err(lookup::history(error)),
        };
        Ok(Self {
            catalog,
            op_id: op,
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
            Err(error) => Err(lookup::history(error)),
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
