use crate::{WorkspaceError, WorkspaceId, WorkspaceResult, WorkspaceSession, WorkspaceSummary};
use layerfs_layerstack_store::{BranchId, LayerStackStore, StoreError};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

const RETAINED_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const SESSION_RETENTION: Retention = Retention {
    count: 256,
    bytes: 1024 * 1024,
    age: RETAINED_AGE,
};
const EXECUTION_RETENTION: Retention = Retention {
    count: 1024,
    bytes: 64 * 1024 * 1024,
    age: RETAINED_AGE,
};

#[derive(Clone)]
pub(crate) enum SessionRecord {
    Active(Arc<crate::worker::WorkspaceWorker>),
    Retained(RetainedSession),
}

#[derive(Clone)]
pub(crate) struct RetainedSession {
    pub(crate) session: WorkspaceSession,
    pub(crate) mutation_generation: u64,
    pub(crate) ended_at: SystemTime,
}

#[derive(Clone, Copy)]
struct Retention {
    count: usize,
    bytes: u64,
    age: Duration,
}

pub struct Workspaces {
    pub(crate) runtime_root: PathBuf,
    pub(crate) store: LayerStackStore,
    pub(crate) sessions: Mutex<BTreeMap<WorkspaceId, SessionRecord>>,
    pub(crate) executions:
        Arc<Mutex<BTreeMap<crate::ExecutionId, Arc<crate::execution::Execution>>>>,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) execution_route: crate::daemon::ExecutionRoute,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) mount_route: crate::daemon::MountRoute,
    pub(crate) daemon: Option<crate::daemon::DaemonOwner>,
}

impl Drop for Workspaces {
    fn drop(&mut self) {
        // Keep each worker and the daemon owner alive until the existing End path
        // acknowledges projection cleanup and removes its state before releasing
        // the branch lease. Socket disconnect alone does not retire a mount.
        let active = match self.sessions.lock() {
            Ok(sessions) => sessions.iter().filter_map(|(id, record)| {
                matches!(record, SessionRecord::Active(_)).then_some(*id)
            }).collect::<Vec<_>>(),
            Err(error) => {
                eprintln!("layerfs-workspace: owner-drop session registry: {error}");
                return;
            }
        };
        for id in active {
            if let Err(error) = self.end_workspace_session(id, crate::EndWorkspaceMode::Discard) {
                // Drop cannot return an error. Preserve the diagnostic and leave
                // unsuccessful state cleanup to the existing projection fallback.
                eprintln!("layerfs-workspace: owner-drop cleanup {id}: {error}");
            }
        }
    }
}

impl Workspaces {
    pub fn new(runtime_root: impl AsRef<Path>, store: LayerStackStore) -> WorkspaceResult<Self> {
        Self::with_daemon(runtime_root, store, crate::daemon::configure()?)
    }

    pub fn new_with_container(
        runtime_root: impl AsRef<Path>,
        store: LayerStackStore,
        binding: crate::ContainerBinding,
    ) -> WorkspaceResult<Self> {
        Self::with_daemon(
            runtime_root,
            store,
            crate::daemon::configure_binding(binding)?,
        )
    }

    fn with_daemon(
        runtime_root: impl AsRef<Path>,
        store: LayerStackStore,
        daemon: crate::daemon::DaemonConfiguration,
    ) -> WorkspaceResult<Self> {
        let runtime_root = runtime_root.as_ref().to_owned();
        std::fs::create_dir_all(&runtime_root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&runtime_root, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self {
            runtime_root,
            store,
            sessions: Mutex::new(BTreeMap::new()),
            executions: Arc::new(Mutex::new(BTreeMap::new())),
            execution_route: daemon.route,
            mount_route: daemon.mount_route,
            daemon: daemon.owner,
        })
    }

    pub(crate) fn worker(
        &self,
        id: WorkspaceId,
    ) -> WorkspaceResult<Arc<crate::worker::WorkspaceWorker>> {
        self.sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .get(&id)
            .and_then(|record| match record {
                SessionRecord::Active(worker) => Some(worker.clone()),
                SessionRecord::Retained(_) => None,
            })
            .ok_or(WorkspaceError::NotFound)
    }

    pub fn active_workspace_count(&self) -> WorkspaceResult<usize> {
        Ok(self
            .sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .values()
            .filter(|record| matches!(record, SessionRecord::Active(_)))
            .count())
    }

    pub fn active_execution_count(&self) -> WorkspaceResult<usize> {
        let executions = self
            .executions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        Ok(executions
            .values()
            .filter(|execution| execution.retention().is_none())
            .count())
    }

    pub(crate) fn workspace_identity(
        &self,
        branch_id: BranchId,
    ) -> WorkspaceResult<crate::worker::WorkspaceIdentity> {
        let branch = self
            .store
            .branch(branch_id)?
            .ok_or(WorkspaceError::NotFound)?;
        let stack =
            self.store
                .layer_stack(branch.layer_stack_id)?
                .ok_or(WorkspaceError::Storage(StoreError::Integrity(
                    "Workspace LayerStack",
                )))?;
        Ok(crate::worker::WorkspaceIdentity {
            layer_stack_id: stack.id,
            layer_stack_name: stack.name,
            branch_name: branch.name,
        })
    }

    pub(crate) fn acquire_lease(
        &self,
        branch_id: BranchId,
    ) -> WorkspaceResult<layerfs_layerstack_store::WorkspaceLease> {
        self.store
            .acquire_workspace_lease(branch_id)?
            .ok_or(WorkspaceError::WorkspaceBusy)
    }

    pub(crate) fn prune_retained(&self) -> WorkspaceResult<()> {
        let now = SystemTime::now();
        let removed_sessions = {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            let entries = sessions
                .iter()
                .filter_map(|(id, record)| match record {
                    SessionRecord::Active(_) => None,
                    SessionRecord::Retained(retained) => {
                        Some((*id, retained.ended_at, retained_session_bytes(retained)))
                    }
                })
                .collect();
            let removed = evictions(entries, SESSION_RETENTION, now);
            sessions.retain(|id, _| !removed.contains(id));
            removed
        };
        let mut executions = self
            .executions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        if !removed_sessions.is_empty() {
            executions.retain(|_, execution| !removed_sessions.contains(&execution.session_id()));
        }
        prune_executions(&mut executions, now);
        Ok(())
    }

    pub fn session_page(
        &self,
        after: Option<WorkspaceId>,
        limit: u16,
    ) -> WorkspaceResult<(Vec<WorkspaceSummary>, Option<WorkspaceId>)> {
        if limit == 0 || limit > 512 {
            return Err(WorkspaceError::InvalidExecution);
        }
        self.prune_retained()?;
        let records = {
            use std::ops::Bound::{Excluded, Unbounded};
            let sessions = self
                .sessions
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            sessions
                .range((after.map_or(Unbounded, Excluded), Unbounded))
                .take(usize::from(limit) + 1)
                .map(|(_, record)| record.clone())
                .collect::<Vec<_>>()
        };
        let has_more = records.len() > usize::from(limit);
        let summaries = records
            .into_iter()
            .take(usize::from(limit))
            .map(record_summary)
            .collect::<WorkspaceResult<Vec<_>>>()?;
        let continuation = has_more
            .then(|| summaries.last().map(|summary| summary.id))
            .flatten();
        Ok((summaries, continuation))
    }
}

fn record_summary(record: SessionRecord) -> WorkspaceResult<WorkspaceSummary> {
    match record {
        SessionRecord::Active(worker) => {
            let broken_cleanup = matches!(
                worker
                    .workspace
                    .lock()
                    .map_err(|_| WorkspaceError::WorkspaceBusy)?
                    .state,
                crate::WorkspaceState::BrokenCleanup
            );
            let dirty = broken_cleanup || crate::projection::is_dirty(&worker)?;
            let workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            Ok(WorkspaceSummary {
                id: worker.id,
                branch_id: workspace.branch_id,
                layer_stack_id: worker.identity.layer_stack_id,
                layer_stack_name: worker.identity.layer_stack_name.clone(),
                branch_name: worker.identity.branch_name.clone(),
                pinned_head: workspace.expected_head,
                state: workspace.state,
                dirty,
            })
        }
        SessionRecord::Retained(retained) => Ok(retained_summary(&retained)),
    }
}

pub(crate) fn prune_execution_registry(
    executions: &Mutex<BTreeMap<crate::ExecutionId, Arc<crate::execution::Execution>>>,
) {
    if let Ok(mut executions) = executions.lock() {
        prune_executions(&mut executions, SystemTime::now());
    }
}

fn prune_executions(
    executions: &mut BTreeMap<crate::ExecutionId, Arc<crate::execution::Execution>>,
    now: SystemTime,
) {
    let entries = executions
        .iter()
        .filter_map(|(id, execution)| {
            execution
                .retention()
                .map(|(completed_at, bytes)| (*id, completed_at, bytes))
        })
        .collect();
    let removed = evictions(entries, EXECUTION_RETENTION, now);
    executions.retain(|id, _| !removed.contains(id));
}

fn retained_session_bytes(retained: &RetainedSession) -> u64 {
    let placement = match &retained.session.placement {
        crate::WorkspacePlacement::Host { root } => root.as_os_str().len(),
        crate::WorkspacePlacement::Container { container_id, root } => {
            container_id.0.len().saturating_add(root.as_os_str().len())
        }
    };
    (std::mem::size_of::<RetainedSession>())
        .saturating_add(placement)
        .try_into()
        .unwrap_or(u64::MAX)
}

fn evictions<K: Copy + Ord>(
    mut entries: Vec<(K, SystemTime, u64)>,
    policy: Retention,
    now: SystemTime,
) -> BTreeSet<K> {
    entries.sort_by_key(|(_, retained_at, _)| *retained_at);
    let mut count = entries.len();
    let mut bytes = entries
        .iter()
        .fold(0_u64, |total, (_, _, bytes)| total.saturating_add(*bytes));
    let mut removed = BTreeSet::new();
    for (id, retained_at, entry_bytes) in entries {
        let expired = now
            .duration_since(retained_at)
            .is_ok_and(|age| age >= policy.age);
        if expired || count > policy.count || bytes > policy.bytes {
            removed.insert(id);
            count -= 1;
            bytes = bytes.saturating_sub(entry_bytes);
        }
    }
    removed
}

pub(crate) fn retained_summary(retained: &RetainedSession) -> WorkspaceSummary {
    WorkspaceSummary {
        id: retained.session.id,
        branch_id: retained.session.branch_id,
        layer_stack_id: retained.session.layer_stack_id,
        layer_stack_name: retained.session.layer_stack_name.clone(),
        branch_name: retained.session.branch_name.clone(),
        pinned_head: retained.session.pinned_head,
        state: retained.session.state,
        dirty: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_evicts_oldest_by_age_count_and_bytes() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let entry = |id, seconds, bytes| (id, now - Duration::from_secs(seconds), bytes);
        assert_eq!(
            evictions(
                vec![entry(1, 11, 1), entry(2, 1, 1)],
                Retention {
                    count: 2,
                    bytes: 2,
                    age: Duration::from_secs(10),
                },
                now,
            ),
            [1].into()
        );
        assert_eq!(
            evictions(
                vec![entry(1, 3, 1), entry(2, 2, 1), entry(3, 1, 1)],
                Retention {
                    count: 2,
                    bytes: 3,
                    age: RETAINED_AGE,
                },
                now,
            ),
            [1].into()
        );
        assert_eq!(
            evictions(
                vec![entry(1, 2, 4), entry(2, 1, 4)],
                Retention {
                    count: 2,
                    bytes: 4,
                    age: RETAINED_AGE,
                },
                now,
            ),
            [1].into()
        );
    }
}
