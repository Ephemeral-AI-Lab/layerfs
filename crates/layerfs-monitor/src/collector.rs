use crate::{
    database_snapshot, DedupAnalysis, MonitorError, MonitorResult, MonitorSnapshot,
    OperationReceipt,
};
use layerfs_layerstack_store::LayerStackStore;
use layerfs_workspace::Workspaces;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const RETAINED_OPERATIONS: usize = 512;

pub struct Monitor {
    store: Arc<LayerStackStore>,
    workspaces: Arc<Workspaces>,
    operations: Mutex<VecDeque<OperationReceipt>>,
    last_analysis: Mutex<Option<DedupAnalysis>>,
}

impl Monitor {
    pub fn new(store: Arc<LayerStackStore>, workspaces: Arc<Workspaces>) -> Self {
        Self {
            store,
            workspaces,
            operations: Mutex::new(VecDeque::new()),
            last_analysis: Mutex::new(None),
        }
    }

    pub fn record(&self, receipt: OperationReceipt) -> MonitorResult<()> {
        if receipt
            .candidate
            .is_some_and(|candidate| !candidate.validate())
        {
            return Err(MonitorError::Integrity("candidate equation"));
        }
        let mut operations = self
            .operations
            .lock()
            .map_err(|_| MonitorError::Integrity("operation receipts"))?;
        operations.push_back(receipt);
        while operations.len() > RETAINED_OPERATIONS {
            operations.pop_front();
        }
        Ok(())
    }

    pub fn snapshot(&self) -> MonitorResult<MonitorSnapshot> {
        Ok(MonitorSnapshot {
            database: database_snapshot(&self.store)?,
            workspaces: self.workspaces.sessions()?,
            operations: self
                .operations
                .lock()
                .map_err(|_| MonitorError::Integrity("operation receipts"))?
                .iter()
                .cloned()
                .collect(),
            last_analysis: self
                .last_analysis
                .lock()
                .map_err(|_| MonitorError::Integrity("dedup analysis"))?
                .clone(),
        })
    }

    pub fn analyze_dedup(&self) -> MonitorResult<DedupAnalysis> {
        let operations = self
            .operations
            .lock()
            .map_err(|_| MonitorError::Integrity("operation receipts"))?
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let analysis = crate::dedup::analyze(&self.store, &operations)?;
        *self
            .last_analysis
            .lock()
            .map_err(|_| MonitorError::Integrity("dedup analysis"))? = Some(analysis.clone());
        Ok(analysis)
    }
}
