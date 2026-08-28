use super::{no_change, valid_empty_root};
use layerfs_core::ObjectId;
use layerfs_durable_store::DurableStore;
use layerfs_storage::integrity::IntegrityMode;
use layerfs_storage::{
    BranchHead, BranchId, LayerId, LayerStackHead, LayerStackId, OperationRecordRef,
};
use layerfs_working_store::{CommitResult, WorkingStore};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct Scenario {
    pub(crate) base: PathBuf,
    pub(crate) working: WorkingStore,
    pub(crate) durable: DurableStore,
    pub(crate) working_b: Option<WorkingStore>,
    pub(crate) root: ObjectId,
    pub(crate) alternate_root: Option<ObjectId>,
    pub(crate) stack: LayerStackHead,
    pub(crate) durable_stack: Option<LayerStackHead>,
    pub(crate) branch_id: BranchId,
    pub(crate) accepted: BranchHead,
    pub(crate) next: Option<BranchHead>,
    pub(crate) continued: Option<BranchHead>,
    pub(crate) continued_record: Option<OperationRecordRef>,
    pub(crate) child: Option<BranchHead>,
    pub(crate) child_head: Option<BranchHead>,
    pub(crate) resumed_head: Option<BranchHead>,
    pub(crate) object_ids: Vec<ObjectId>,
}

impl Scenario {
    pub(crate) fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "layerfs-sync-branch-push-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&base).unwrap();
        let mut working =
            WorkingStore::open(&base.join("working"), IntegrityMode::TrustedLocalDev).unwrap();
        let root = valid_empty_root(&mut working);
        let stack = working
            .create_layer_stack(
                LayerStackId::from_bytes([0x41; 32]),
                LayerId::from_bytes([0x42; 32]),
                "main",
                root,
            )
            .unwrap();
        let branch = working
            .create_top_level_branch(BranchId::from_bytes([0x43; 32]), Some("work"), stack)
            .unwrap();
        let begin = working.begin_operation(branch).unwrap();
        let accepted = match working
            .operation_commit(begin, no_change(&begin, root))
            .unwrap()
        {
            CommitResult::WorkingRecorded { head, .. } => head,
            CommitResult::Conflict { .. } => panic!("unexpected Working conflict"),
        };
        let object_ids = object_ids(&working);
        let durable = DurableStore::open(&base.join("durable")).unwrap();
        Self {
            base,
            working,
            durable,
            working_b: None,
            root,
            alternate_root: None,
            stack,
            durable_stack: None,
            branch_id: branch.branch_id,
            accepted,
            next: None,
            continued: None,
            continued_record: None,
            child: None,
            child_head: None,
            resumed_head: None,
            object_ids,
        }
    }

    pub(crate) fn cleanup(self) {
        let base = self.base.clone();
        drop(self);
        fs::remove_dir_all(base).unwrap();
    }
}

pub(crate) fn object_ids(working: &WorkingStore) -> Vec<ObjectId> {
    let mut ids = Vec::new();
    let mut after = None;
    loop {
        let page = working.object_ids_page(after, 16).unwrap();
        if page.is_empty() {
            return ids;
        }
        after = page.last().copied();
        ids.extend(page);
    }
}
