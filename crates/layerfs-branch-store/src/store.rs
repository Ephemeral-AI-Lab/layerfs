use layerfs_content::ObjectId;
use layerfs_storage::{BranchId, CommitId, Result, StorageError, StoreDb, StoreId, StoreRole};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct BranchStore {
    pub(crate) db: StoreDb,
    workspace_leases: Arc<Mutex<BTreeSet<BranchId>>>,
    push_plans: Arc<Mutex<BTreeMap<BranchId, PushPlan>>>,
}

#[derive(Clone)]
pub(crate) struct PushPlan {
    pub commit_id: CommitId,
    pub base_root: ObjectId,
    pub new_root: ObjectId,
    pub ids: Vec<ObjectId>,
}

const PUSH_PLAN_ENTRIES: usize = 64;
pub(crate) const PUSH_PLAN_ID_CAP: usize = 32_768;

impl BranchStore {
    pub fn create(path: impl AsRef<Path>, parent_store_id: StoreId) -> Result<Self> {
        Ok(Self {
            db: StoreDb::create(path, StoreRole::Branch, Some(parent_store_id))?,
            workspace_leases: Arc::new(Mutex::new(BTreeSet::new())),
            push_plans: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    pub fn connect(path: impl AsRef<Path>, expected_parent_store_id: StoreId) -> Result<Self> {
        Ok(Self {
            db: StoreDb::connect(path, StoreRole::Branch, Some(expected_parent_store_id))?,
            workspace_leases: Arc::new(Mutex::new(BTreeSet::new())),
            push_plans: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    pub fn path(&self) -> &Path {
        self.db.path()
    }

    pub fn store_id(&self) -> StoreId {
        self.db.store_id()
    }

    pub fn parent_store_id(&self) -> StoreId {
        self.db.parent_store_id().expect("BranchStore parent")
    }

    #[doc(hidden)]
    pub fn acquire_workspace_lease(&self, branch_id: BranchId) -> Result<bool> {
        self.workspace_leases
            .lock()
            .map_err(|_| StorageError::Integrity("Workspace lease"))
            .map(|mut leases| leases.insert(branch_id))
    }

    #[doc(hidden)]
    pub fn release_workspace_lease(&self, branch_id: BranchId) {
        if let Ok(mut leases) = self.workspace_leases.lock() {
            leases.remove(&branch_id);
        }
    }

    pub(crate) fn retain_push_plan(&self, branch_id: BranchId, plan: PushPlan) {
        if plan.ids.len() > PUSH_PLAN_ID_CAP {
            return;
        }
        let Ok(mut plans) = self.push_plans.lock() else {
            return;
        };
        let mut total_ids = plans.values().map(|plan| plan.ids.len()).sum::<usize>();
        if let Some(previous) = plans.remove(&branch_id) {
            total_ids = total_ids.saturating_sub(previous.ids.len());
        }
        while plans.len() >= PUSH_PLAN_ENTRIES
            || total_ids.saturating_add(plan.ids.len()) > PUSH_PLAN_ID_CAP
        {
            let Some(evicted) = plans.keys().next().copied() else {
                break;
            };
            if let Some(evicted) = plans.remove(&evicted) {
                total_ids = total_ids.saturating_sub(evicted.ids.len());
            }
        }
        plans.insert(branch_id, plan);
    }

    pub(crate) fn push_plan(&self, branch_id: BranchId) -> Option<PushPlan> {
        self.push_plans
            .lock()
            .ok()
            .and_then(|plans| plans.get(&branch_id).cloned())
    }

    pub(crate) fn clear_push_plan(&self, branch_id: BranchId, commit_id: CommitId) {
        if let Ok(mut plans) = self.push_plans.lock() {
            if plans.get(&branch_id).map(|plan| plan.commit_id) == Some(commit_id) {
                plans.remove(&branch_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_push_plan_is_rejected_for_safe_fallback() {
        let path = std::env::temp_dir().join(format!(
            "layerfs-push-plan-cap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = BranchStore::create(&path, StoreId::from_bytes([7; 32])).unwrap();
        let branch_id = BranchId::new();
        let root = ObjectId::for_bytes(b"root");
        let base =
            layerfs_storage::LayerId::derive(layerfs_storage::LayerStackId::new(), None, root);
        let commit_id = CommitId::derive(root, None, base);
        let plan = |count: usize| PushPlan {
            commit_id,
            base_root: root,
            new_root: root,
            ids: (0..count)
                .map(|index| ObjectId::for_bytes(&(index as u64).to_be_bytes()))
                .collect(),
        };

        store.retain_push_plan(branch_id, plan(PUSH_PLAN_ID_CAP + 1));
        assert!(store.push_plan(branch_id).is_none());
        store.retain_push_plan(branch_id, plan(PUSH_PLAN_ID_CAP));
        assert_eq!(
            store.push_plan(branch_id).unwrap().ids.len(),
            PUSH_PLAN_ID_CAP
        );

        drop(store);
        std::fs::remove_file(path).unwrap();
    }
}
