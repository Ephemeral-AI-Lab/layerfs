use crate::{
    cow_tree::WorkspaceSnapshot, worker::WorkspaceWorker, CreateWorkspaceSession, ResourcePolicy,
    Workspace, WorkspaceError, WorkspaceId, WorkspacePlacement, WorkspaceProjection,
    WorkspaceResult, Workspaces,
};
use layerfs_content::CanonicalPath;
use layerfs_layerstack_store::{
    BranchId, LayerId, LayerStackStore, PreparedReconciliation, ReconcileChoice,
    ReconcileConflictKind, Result as StoreResult, StoreError,
};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConflictId([u8; 16]);

impl ConflictId {
    fn new(index: usize, workspace_id: WorkspaceId) -> Self {
        let mut bytes = workspace_id.to_string().into_bytes();
        bytes.extend_from_slice(&index.to_be_bytes());
        let digest = layerfs_content::ObjectId::for_bytes(&bytes).to_bytes();
        Self(digest[..16].try_into().expect("fixed ConflictId"))
    }
}

impl fmt::Display for ConflictId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("c:")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for ConflictId {
    type Err = WorkspaceError;

    fn from_str(value: &str) -> WorkspaceResult<Self> {
        let value = value
            .strip_prefix("c:")
            .ok_or(WorkspaceError::InvalidExecution)?;
        if value.len() != 32 {
            return Err(WorkspaceError::InvalidExecution);
        }
        let mut bytes = [0; 16];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex(pair[0])? << 4) | hex(pair[1])?;
        }
        Ok(Self(bytes))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictKind {
    Content,
    Type,
    Directory,
    HardLink,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceConflict {
    pub conflict_id: ConflictId,
    pub kind: ConflictKind,
    pub affected_paths: Vec<CanonicalPath>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConflictCursor(pub u64);

impl fmt::Display for ConflictCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::str::FromStr for ConflictCursor {
    type Err = WorkspaceError;

    fn from_str(value: &str) -> WorkspaceResult<Self> {
        value
            .parse()
            .map(Self)
            .map_err(|_| WorkspaceError::InvalidExecution)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictPage {
    pub conflicts: Vec<WorkspaceConflict>,
    pub continuation: Option<ConflictCursor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolveChoice {
    Branch,
    Layer,
    WorkingTree,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolveResult {
    pub conflict_id: ConflictId,
    pub remaining: u64,
}

pub(crate) struct ResolutionState {
    pub(crate) prepared: PreparedReconciliation,
    conflicts: Vec<ResolutionConflict>,
}

struct ResolutionConflict {
    value: WorkspaceConflict,
    choice: Option<ResolveChoice>,
    resolved_generation: Option<u64>,
    resolved_fingerprint: Option<[u8; 32]>,
}

impl ResolutionState {
    fn new(id: WorkspaceId, prepared: PreparedReconciliation) -> Self {
        let conflicts = prepared
            .conflicts
            .iter()
            .enumerate()
            .map(|(index, conflict)| ResolutionConflict {
                value: WorkspaceConflict {
                    conflict_id: ConflictId::new(index, id),
                    kind: match conflict.kind {
                        ReconcileConflictKind::Content => ConflictKind::Content,
                        ReconcileConflictKind::Type => ConflictKind::Type,
                        ReconcileConflictKind::Directory => ConflictKind::Directory,
                        ReconcileConflictKind::HardLink => ConflictKind::HardLink,
                    },
                    affected_paths: conflict.affected_paths.clone(),
                },
                choice: None,
                resolved_generation: None,
                resolved_fingerprint: None,
            })
            .collect();
        Self {
            prepared,
            conflicts,
        }
    }

    pub(crate) fn invalidate_if_mutated(&mut self, workspace: &mut Workspace) -> StoreResult<()> {
        for conflict in &mut self.conflicts {
            conflict.invalidate_if_mutated(workspace)?;
        }
        Ok(())
    }

    pub(crate) fn unresolved(&self) -> usize {
        self.conflicts
            .iter()
            .filter(|conflict| conflict.choice.is_none())
            .count()
    }

    pub(crate) fn choices(&self) -> StoreResult<Vec<ReconcileChoice>> {
        self.conflicts
            .iter()
            .map(|conflict| match conflict.choice {
                Some(ResolveChoice::Branch) => Ok(ReconcileChoice::Branch),
                Some(ResolveChoice::Layer) => Ok(ReconcileChoice::Layer),
                Some(ResolveChoice::WorkingTree) => Ok(ReconcileChoice::WorkingTree),
                None => Err(StoreError::InvalidInput(
                    "unresolved reconciliation conflict",
                )),
            })
            .collect()
    }
}

impl ResolutionConflict {
    fn invalidate_if_mutated(&mut self, workspace: &mut Workspace) -> StoreResult<()> {
        let Some(resolved) = self.resolved_generation else {
            return Ok(());
        };
        let path_was_mutated = self.path_was_mutated(&workspace.mutation_paths, resolved);
        let fingerprint_changed = match self.resolved_fingerprint {
            Some(expected) => {
                workspace.resolution_fingerprint(&self.value.affected_paths)? != expected
            }
            None => false,
        };
        if path_was_mutated || fingerprint_changed {
            self.choice = None;
            self.resolved_generation = None;
            self.resolved_fingerprint = None;
        }
        Ok(())
    }

    fn path_was_mutated(&self, mutations: &BTreeMap<String, u64>, resolved: u64) -> bool {
        mutations.iter().any(|(path, generation)| {
            *generation > resolved
                && self
                    .value
                    .affected_paths
                    .iter()
                    .any(|affected| paths_intersect(path, affected.as_str()))
        })
    }
}

impl Workspace {
    fn open_resolution(
        store: LayerStackStore,
        id: WorkspaceId,
        prepared: PreparedReconciliation,
        spool: &std::path::Path,
    ) -> StoreResult<Self> {
        let reader = store.reconciliation_reader(&prepared);
        let mut workspace = Self::from_snapshot(
            WorkspaceSnapshot {
                store,
                branch_id: prepared.branch_id,
                expected_head: Some(prepared.expected_head),
                expected_base: prepared.old_base_layer_id,
                root: prepared.root_id,
                reader,
            },
            spool,
            ResourcePolicy::default(),
        )?;
        workspace.resolution = Some(ResolutionState::new(id, prepared));
        Ok(workspace)
    }
}

impl Workspaces {
    pub fn create_reconciliation_workspace(
        &self,
        branch_id: BranchId,
        current_layer_id: LayerId,
    ) -> WorkspaceResult<(WorkspaceId, u64)> {
        if let Some(existing) = self.existing_reconciliation(branch_id, current_layer_id)? {
            return Ok(existing);
        }
        let lease = self.acquire_lease(branch_id)?;
        let prepared = self
            .store
            .prepare_reconciliation(branch_id, current_layer_id)?;
        let conflict_count = prepared.conflicts.len() as u64;
        let id = WorkspaceId::new();
        let state = self.runtime_root.join("workspaces").join(id.to_string());
        let root = state.join("view");
        std::fs::create_dir_all(&state)?;
        let request = CreateWorkspaceSession {
            branch_id,
            placement: WorkspacePlacement::Host { root },
            projection: Some(WorkspaceProjection::Materialize),
        };
        let workspace =
            Workspace::open_resolution(self.store.clone(), id, prepared, &state.join("spool"))?;
        let identity = self.workspace_identity(branch_id)?;
        let worker = Arc::new(WorkspaceWorker::new(
            id,
            request,
            WorkspaceProjection::Materialize,
            identity,
            workspace,
            lease,
        ));
        let handle = crate::projection::attach(&worker, None)?;
        *worker
            .projection_handle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)? = Some(handle);
        self.sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .insert(id, crate::registry::SessionRecord::Active(worker));
        Ok((id, conflict_count))
    }

    fn existing_reconciliation(
        &self,
        branch_id: BranchId,
        current_layer_id: LayerId,
    ) -> WorkspaceResult<Option<(WorkspaceId, u64)>> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        for (id, record) in sessions.iter() {
            let crate::registry::SessionRecord::Active(worker) = record else {
                continue;
            };
            if worker.request.branch_id != branch_id {
                continue;
            }
            let workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            if let Some(resolution) = &workspace.resolution {
                if resolution.prepared.current_layer_id == current_layer_id {
                    return Ok(Some((*id, resolution.conflicts.len() as u64)));
                }
            }
        }
        Ok(None)
    }

    pub fn workspace_conflicts(
        &self,
        workspace_id: WorkspaceId,
        cursor: Option<ConflictCursor>,
    ) -> WorkspaceResult<ConflictPage> {
        let worker = self.worker(workspace_id)?;
        let workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let resolution = workspace
            .resolution
            .as_ref()
            .ok_or(WorkspaceError::InvalidExecution)?;
        let start = cursor.map_or(0, |cursor| cursor.0 as usize);
        let conflicts = resolution
            .conflicts
            .iter()
            .skip(start)
            .take(128)
            .map(|conflict| conflict.value.clone())
            .collect::<Vec<_>>();
        let next = start + conflicts.len();
        Ok(ConflictPage {
            conflicts,
            continuation: (next < resolution.conflicts.len())
                .then_some(ConflictCursor(next as u64)),
        })
    }

    pub fn resolve_workspace_conflict(
        &self,
        workspace_id: WorkspaceId,
        conflict_id: ConflictId,
        choice: ResolveChoice,
    ) -> WorkspaceResult<ResolveResult> {
        let worker = self.worker(workspace_id)?;
        crate::projection::pause(&worker)?;
        let result = (|| {
            let _quiesced = worker.quiesce()?;
            crate::projection::capture(&worker)?;
            let mut workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            if choice == ResolveChoice::WorkingTree {
                workspace.build_candidate()?;
            }
            let affected_paths = workspace
                .resolution
                .as_ref()
                .ok_or(WorkspaceError::InvalidExecution)?
                .conflicts
                .iter()
                .find(|conflict| conflict.value.conflict_id == conflict_id)
                .ok_or(WorkspaceError::NotFound)?
                .value
                .affected_paths
                .clone();
            let fingerprint = workspace.resolution_fingerprint(&affected_paths)?;
            let generation = workspace.mutation_generation;
            let resolution = workspace
                .resolution
                .as_mut()
                .ok_or(WorkspaceError::InvalidExecution)?;
            let conflict = resolution
                .conflicts
                .iter_mut()
                .find(|conflict| conflict.value.conflict_id == conflict_id)
                .ok_or(WorkspaceError::NotFound)?;
            conflict.choice = Some(choice);
            conflict.resolved_generation = Some(generation);
            conflict.resolved_fingerprint = Some(fingerprint);
            Ok(ResolveResult {
                conflict_id,
                remaining: resolution.unresolved() as u64,
            })
        })();
        crate::projection::resume(&worker)?;
        result
    }
}

fn paths_intersect(left: &str, right: &str) -> bool {
    left == right || is_ancestor(left, right) || is_ancestor(right, left)
}

fn is_ancestor(parent: &str, child: &str) -> bool {
    parent.is_empty()
        || (child.starts_with(parent) && child.as_bytes().get(parent.len()) == Some(&b'/'))
}

fn hex(value: u8) -> WorkspaceResult<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(WorkspaceError::InvalidExecution),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_intersecting_later_mutations_invalidate_a_resolution() {
        let conflict = || ResolutionConflict {
            value: WorkspaceConflict {
                conflict_id: ConflictId([0; 16]),
                kind: ConflictKind::Directory,
                affected_paths: vec![CanonicalPath::new("dir/file").unwrap()],
            },
            choice: Some(ResolveChoice::WorkingTree),
            resolved_generation: Some(4),
            resolved_fingerprint: Some([1; 32]),
        };
        let unrelated = conflict();
        assert!(!unrelated.path_was_mutated(&BTreeMap::from([("other".to_owned(), 5)]), 4));

        for path in ["dir/file", "dir", "dir/file/child"] {
            let affected = conflict();
            assert!(
                affected.path_was_mutated(&BTreeMap::from([(path.to_owned(), 5)]), 4),
                "{path}"
            );
        }

        let older = conflict();
        assert!(!older.path_was_mutated(&BTreeMap::from([("dir/file".to_owned(), 4)]), 4));
    }
}
