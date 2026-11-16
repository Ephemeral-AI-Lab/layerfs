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

/// A `branch.remove` intent at Branch scope.
pub fn branch_removal(stack: &LayerStackRecord, branch: &BranchRecord) -> GcIntent {
    GcIntent::operation(
        op_id(),
        GcIntentKind::Removal,
        GcScopeKind::Branch,
        branch.id.as_slice(),
    )
    .in_project(stack.id.as_slice())
    .named(Some(stack.name.as_str()), Some(branch.name.as_str()))
}

/// A `project.delete` or `project.remove_file` intent at project scope.
pub fn project_removal(stack: &LayerStackRecord) -> GcIntent {
    GcIntent::operation(
        op_id(),
        GcIntentKind::Removal,
        GcScopeKind::Project,
        stack.id.as_slice(),
    )
    .named(Some(stack.name.as_str()), None)
}

/// A `project.delete` intent when only the stack id is known.
pub fn project_removal_from_id(stack: LayerStackId) -> GcIntent {
    project_removal_at(stack, op_id())
}

/// A `project.delete` intent under an operation identity the caller already
/// registered. The control layer admits the project scope before its sidecar
/// and marker and reuses the identity there, so one row is taken and released
/// once, never two competing rows.
pub fn project_removal_at(stack: LayerStackId, op: [u8; 16]) -> GcIntent {
    GcIntent::operation(
        op,
        GcIntentKind::Removal,
        GcScopeKind::Project,
        stack.as_slice(),
    )
}

/// A mount intent at Branch scope, naming its project identity for the
/// PostgreSQL key derivation. The Workspace is always placed; the Sandbox
/// and machine are placed only when the caller that mounted knows them
/// (a daemon mount does; a private Store-call Workspace does not).
pub fn mount_intent(
    workspace: layerfs_history::WorkspaceId,
    snapshot: &layerfs_history::BranchSnapshot,
    place: &layerfs_bridge::control::MountPlace,
) -> GcIntent {
    let body: [u8; 32] = workspace.as_slice().try_into().unwrap_or([0u8; 32]);
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
            .named(Some(stack.name.as_str()), branch);
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
            let _ = self.catalog.gc_release(&self.op_id);
        }
    }
}
