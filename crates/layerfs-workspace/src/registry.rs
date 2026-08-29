use crate::{
    WorkspaceError, WorkspaceResult, WorkspaceSession, WorkspaceSessionId, WorkspaceSummary,
};
use layerfs_branch_store::BranchStore;
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
    pub(crate) branches: Mutex<Vec<BranchStore>>,
    pub(crate) sessions: Mutex<BTreeMap<WorkspaceSessionId, SessionRecord>>,
    pub(crate) executions:
        Arc<Mutex<BTreeMap<crate::ExecutionId, Arc<crate::execution::Execution>>>>,
}

impl Workspaces {
    pub fn new(
        runtime_root: impl AsRef<Path>,
        branches: impl IntoIterator<Item = BranchStore>,
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
            branches: Mutex::new(branches.into_iter().collect()),
            sessions: Mutex::new(BTreeMap::new()),
            executions: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    pub(crate) fn worker(
        &self,
        id: WorkspaceSessionId,
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

    #[doc(hidden)]
    pub fn attach_branch_store(&self, store: BranchStore) -> WorkspaceResult<()> {
        let mut branches = self
            .branches
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        if branches.iter().all(|branch| branch.path() != store.path()) {
            branches.push(store);
        }
        Ok(())
    }

    #[doc(hidden)]
    pub fn detach_branch_store(&self, path: &Path) -> WorkspaceResult<()> {
        let mut branches = self
            .branches
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let Some(position) = branches.iter().position(|branch| branch.path() == path) else {
            return Err(WorkspaceError::NotFound);
        };
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        for session in sessions.values() {
            if let SessionRecord::Active(session) = session {
                if branches[position]
                    .branch(session.request.branch_id)?
                    .is_some()
                {
                    return Err(WorkspaceError::WorkspaceBusy);
                }
            }
        }
        drop(sessions);
        branches.remove(position);
        Ok(())
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
