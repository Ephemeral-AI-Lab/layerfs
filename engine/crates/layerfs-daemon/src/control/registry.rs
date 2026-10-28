//! Short control admission over bounded daemon-owned Workspace routes.
use super::{
    kernel::{Kernel, KernelServing},
    Failure,
};
use crate::{
    store::{BoundWorkspace, Store},
    Owner, OwnerClient,
};
use layerfs_bridge::control::{Activity, ControlCode, WorkspaceToken};
use layerfs_history::{BranchSnapshot, CommitOutcome, WorkspaceId};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
pub(super) struct Binding {
    pub kernel: Kernel,
    pub workspace: Arc<BoundWorkspace>,
    pub snapshot: BranchSnapshot,
    pub activity: Activity,
    pub epoch: u64,
    pub published: Option<CommitOutcome>,
}
pub(super) enum Entry {
    Binding,
    Bound(Box<Binding>),
}
/// One daemon's kernel control owner. Whole operations run outside its registry mutex.
pub struct Service {
    pub(crate) store: Arc<Store>,
    pub(super) owner: OwnerClient,
    pub(super) capacity: usize,
    pub(super) entries: Mutex<BTreeMap<WorkspaceId, Entry>>,
    /// Absent for a control-only Service: Attach is then refused before effects.
    pub(super) kernel: Option<Arc<KernelServing>>,
}
impl Service {
    /// Binds control to the already initialized owner and its configured capacity.
    pub fn new(store: Arc<Store>, owner: &Owner) -> Self {
        Self {
            store,
            owner: owner.client(),
            capacity: owner.configuration().namespaces,
            entries: Mutex::new(BTreeMap::new()),
            kernel: None,
        }
    }
    /// Binds control and kernel serving to the same Store and overlay owner.
    pub fn with_kernel(store: Arc<Store>, owner: &Owner, kernel: Arc<KernelServing>) -> Self {
        Self {
            kernel: Some(kernel),
            ..Self::new(store, owner)
        }
    }
    /// Retains the selected binding for ordinary local projection/operations.
    /// Terminal admission prevents new transfers; stale tokens never redirect.
    pub fn operation(
        &self,
        token: WorkspaceToken,
    ) -> Result<crate::store::StoreOperation, Failure> {
        let workspace = {
            let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
            let value = bound(&entries, token)?;
            if value.activity == Activity::Closing {
                return Err(Failure::Rejected(ControlCode::Busy, "Workspace is closing"));
            }
            value.workspace.clone()
        };
        workspace.operation().map_err(Failure::Workspace)
    }
    /// Workspaces bound or binding; None when the registry is poisoned.
    pub fn workspaces(&self) -> Option<usize> {
        self.entries.lock().ok().map(|entries| entries.len())
    }
    pub(super) fn admit(
        &self,
        token: WorkspaceToken,
        activity: Activity,
    ) -> Result<Arc<BoundWorkspace>, Failure> {
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        let value = bound_mut(&mut entries, token)?;
        idle(value)?;
        value.activity = activity;
        value.epoch = value.epoch.saturating_add(1);
        Ok(value.workspace.clone())
    }
}
pub(super) fn bound(
    entries: &BTreeMap<WorkspaceId, Entry>,
    token: WorkspaceToken,
) -> Result<&Binding, Failure> {
    let value = match entries.get(&token.workspace) {
        Some(Entry::Bound(value)) => value,
        Some(Entry::Binding) => {
            return Err(Failure::Rejected(
                ControlCode::Busy,
                "Workspace binding retained",
            ))
        }
        None => return Err(Failure::Rejected(ControlCode::Missing, "Workspace absent")),
    };
    check(value, token)?;
    Ok(value)
}
pub(super) fn bound_mut(
    entries: &mut BTreeMap<WorkspaceId, Entry>,
    token: WorkspaceToken,
) -> Result<&mut Binding, Failure> {
    let value = match entries.get_mut(&token.workspace) {
        Some(Entry::Bound(value)) => value,
        Some(Entry::Binding) => {
            return Err(Failure::Rejected(
                ControlCode::Busy,
                "Workspace binding retained",
            ))
        }
        None => return Err(Failure::Rejected(ControlCode::Missing, "Workspace absent")),
    };
    check(value, token)?;
    Ok(value)
}
fn check(value: &Binding, token: WorkspaceToken) -> Result<(), Failure> {
    if value.workspace.route().namespace() != token.namespace {
        return Err(Failure::Rejected(
            ControlCode::Invalid,
            "stale Workspace namespace",
        ));
    }
    Ok(())
}

pub(super) fn idle(value: &Binding) -> Result<(), Failure> {
    match value.activity {
        Activity::Idle => Ok(()),
        Activity::Uncertain | Activity::LocalFailure => Err(Failure::Custody(Box::new(
            layerfs_bridge::control::ControlRefusal {
                code: ControlCode::Unknown,
                phase: "admission".into(),
                moved: None,
                published: value.published.clone(),
                detail: "original Commit custody retained".into(),
            },
        ))),
        _ => Err(Failure::Rejected(
            ControlCode::Busy,
            "Workspace control operation active",
        )),
    }
}
