use crate::resource::process_snapshot;
use crate::retention::Retention;
use crate::timing::Histogram;
use crate::{
    DatabaseSnapshot, DedupAnalysis, MonitorError, MonitorResult, MonitorSnapshot, OperationReceipt,
};
use layerfs_branch_store::BranchStore;
use layerfs_layerstack_store::LayerStackStore;
use layerfs_workspace::Workspaces;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

const RETAINED_RECEIPTS: usize = 10_000;
const RETAINED_RECEIPT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
struct ReceiptBuffer {
    items: VecDeque<(usize, OperationReceipt)>,
    bytes: usize,
}

pub struct Monitor {
    layerstack: Arc<LayerStackStore>,
    branch: BranchStore,
    workspaces: Arc<Workspaces>,
    receipts: Mutex<ReceiptBuffer>,
    timing: Mutex<Histogram>,
    dedup: Mutex<Option<DedupAnalysis>>,
    retention: Retention,
}

impl Monitor {
    pub fn new(
        runtime_root: impl AsRef<Path>,
        layerstack: Arc<LayerStackStore>,
        branch: BranchStore,
        workspaces: Arc<Workspaces>,
    ) -> MonitorResult<Self> {
        let runtime_root = runtime_root.as_ref();
        std::fs::create_dir_all(runtime_root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(runtime_root, std::fs::Permissions::from_mode(0o700))?;
        }
        let retention = Retention::new(runtime_root)?;
        let retained = retention.load()?;
        let mut receipts = ReceiptBuffer::default();
        for receipt in retained {
            let bytes = receipt.to_json().len();
            receipts.bytes = receipts.bytes.saturating_add(bytes);
            receipts.items.push_back((bytes, receipt));
        }
        while receipts.items.len() > RETAINED_RECEIPTS || receipts.bytes > RETAINED_RECEIPT_BYTES {
            let Some((bytes, _)) = receipts.items.pop_front() else {
                break;
            };
            receipts.bytes = receipts.bytes.saturating_sub(bytes);
        }
        Ok(Self {
            layerstack,
            branch,
            workspaces,
            receipts: Mutex::new(receipts),
            timing: Mutex::new(Histogram::default()),
            dedup: Mutex::new(None),
            retention,
        })
    }

    pub fn snapshot(&self) -> MonitorResult<MonitorSnapshot> {
        let operations = self
            .receipts
            .lock()
            .map_err(|_| MonitorError::Integrity("receipt lock"))?
            .items
            .iter()
            .map(|(_, receipt)| receipt.clone())
            .collect();
        let snapshot = MonitorSnapshot {
            databases: vec![
                DatabaseSnapshot {
                    role: "layerstack".to_owned(),
                    location: self.layerstack.path().to_string_lossy().into_owned(),
                    storage: self.layerstack.storage_snapshot()?,
                },
                DatabaseSnapshot {
                    role: "branch".to_owned(),
                    location: self.branch.path().to_string_lossy().into_owned(),
                    storage: self.branch.storage_snapshot()?,
                },
            ],
            workspaces: self.workspaces.sessions()?,
            operations,
            dedup: self
                .dedup
                .lock()
                .map_err(|_| MonitorError::Integrity("dedup lock"))?
                .clone(),
            process_id: 0,
            resident_bytes: None,
            available_parallelism: 0,
        };
        Ok(snapshot.with_process(process_snapshot()))
    }

    pub fn analyze_dedup(&self) -> MonitorResult<DedupAnalysis> {
        let operations = self
            .receipts
            .lock()
            .map_err(|_| MonitorError::Integrity("receipt lock"))?
            .items
            .iter()
            .map(|(_, receipt)| receipt.clone())
            .collect::<Vec<_>>();
        let analysis = crate::dedup::analyze(&self.layerstack, &self.branch, &operations)?;
        *self
            .dedup
            .lock()
            .map_err(|_| MonitorError::Integrity("dedup lock"))? = Some(analysis.clone());
        Ok(analysis)
    }

    pub fn record(&self, receipt: OperationReceipt) -> MonitorResult<()> {
        if !receipt.timing_is_consistent() {
            return Err(MonitorError::Integrity("operation timing"));
        }
        self.retention.append(&receipt)?;
        self.timing
            .lock()
            .map_err(|_| MonitorError::Integrity("timing lock"))?
            .observe(receipt.service_ns);
        let bytes = receipt.to_json().len();
        let mut receipts = self
            .receipts
            .lock()
            .map_err(|_| MonitorError::Integrity("receipt lock"))?;
        receipts.bytes = receipts.bytes.saturating_add(bytes);
        receipts.items.push_back((bytes, receipt));
        while receipts.items.len() > RETAINED_RECEIPTS || receipts.bytes > RETAINED_RECEIPT_BYTES {
            let Some((bytes, _)) = receipts.items.pop_front() else {
                break;
            };
            receipts.bytes = receipts.bytes.saturating_sub(bytes);
        }
        Ok(())
    }

    pub fn begin_operation(&self) {
        drop(layerfs_storage::take_storage_receipts());
    }

    pub fn finish_operation(&self) -> Vec<layerfs_storage::StorageReceipt> {
        layerfs_storage::take_storage_receipts()
    }
}
