use crate::resource::process_snapshot;
use crate::retention::Retention;
use crate::timing::Histogram;
use crate::{
    BranchStoreId, DatabaseSnapshot, DedupAnalysis, MonitorError, MonitorResult, MonitorScope,
    MonitorSnapshot, MonitoredRoute, OperationReceipt,
};
use layerfs_storage::StoreEndpoint;
use layerfs_workspace::Workspaces;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
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
    routes: Mutex<BTreeMap<BranchStoreId, MonitoredRoute>>,
    workspaces: Arc<Workspaces>,
    receipts: Mutex<ReceiptBuffer>,
    timing: Mutex<Histogram>,
    dedup: Mutex<BTreeMap<BranchStoreId, DedupAnalysis>>,
    retention: Retention,
}

impl Monitor {
    pub fn new(
        runtime_root: impl AsRef<Path>,
        routes: impl IntoIterator<Item = MonitoredRoute>,
        workspaces: Arc<Workspaces>,
    ) -> MonitorResult<Self> {
        let runtime_root = runtime_root.as_ref();
        std::fs::create_dir_all(runtime_root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(runtime_root, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self {
            routes: Mutex::new(routes.into_iter().map(|route| (route.id, route)).collect()),
            workspaces,
            receipts: Mutex::new(ReceiptBuffer::default()),
            timing: Mutex::new(Histogram::default()),
            dedup: Mutex::new(BTreeMap::new()),
            retention: Retention::new(runtime_root)?,
        })
    }

    pub fn snapshot(&self, scope: MonitorScope) -> MonitorResult<MonitorSnapshot> {
        match scope {
            MonitorScope::Databases => self.database_snapshot(),
            MonitorScope::Dedup { route } => {
                let analyses = self
                    .dedup
                    .lock()
                    .map_err(|_| MonitorError::Integrity("dedup lock"))?;
                Ok(MonitorSnapshot::Dedup(
                    analyses
                        .iter()
                        .filter(|(id, _)| route.is_none_or(|route| route == **id))
                        .map(|(id, analysis)| (*id, analysis.clone()))
                        .collect(),
                ))
            }
            MonitorScope::Workspace(id) => {
                let sessions = self.workspaces.sessions()?;
                Ok(MonitorSnapshot::Workspaces(
                    sessions
                        .into_iter()
                        .filter(|session| id.is_none_or(|id| session.id == id))
                        .collect(),
                ))
            }
            MonitorScope::Branch(id) => {
                let routes = self
                    .routes
                    .lock()
                    .map_err(|_| MonitorError::Integrity("route lock"))?;
                for route in routes.values() {
                    if let Some(branch) = route.branch.branch(id)? {
                        return Ok(MonitorSnapshot::Branch(Some(branch)));
                    }
                }
                Ok(MonitorSnapshot::Branch(None))
            }
            MonitorScope::Operation(id) => {
                let receipts = self
                    .receipts
                    .lock()
                    .map_err(|_| MonitorError::Integrity("receipt lock"))?;
                Ok(MonitorSnapshot::Operations(
                    receipts
                        .items
                        .iter()
                        .map(|(_, receipt)| receipt)
                        .filter(|receipt| id.is_none_or(|id| receipt.id == id))
                        .cloned()
                        .collect(),
                ))
            }
            MonitorScope::Process => Ok(process_snapshot().into()),
        }
    }

    pub fn analyze_dedup(&self, route: BranchStoreId) -> MonitorResult<DedupAnalysis> {
        let route = self
            .routes
            .lock()
            .map_err(|_| MonitorError::Integrity("route lock"))?
            .get(&route)
            .cloned()
            .ok_or(MonitorError::NotFound)?;
        let (union_cas_bytes, placements) = route.inventories()?;
        let route_cas_bytes = placements
            .iter()
            .try_fold(0_u64, |total, placement| {
                total.checked_add(placement.encoded_bytes)
            })
            .ok_or(MonitorError::Integrity("route bytes"))?;
        let cross_store_placement_bytes = route_cas_bytes.saturating_sub(union_cas_bytes);
        let analysis = DedupAnalysis {
            route_cas_bytes,
            union_cas_bytes,
            cross_store_placement_bytes,
            placement_factor: if union_cas_bytes == 0 {
                1.0
            } else {
                route_cas_bytes as f64 / union_cas_bytes as f64
            },
            placements,
        };
        self.dedup
            .lock()
            .map_err(|_| MonitorError::Integrity("dedup lock"))?
            .insert(route.id, analysis.clone());
        Ok(analysis)
    }

    #[doc(hidden)]
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

    #[doc(hidden)]
    pub fn begin_operation(&self) {
        drop(layerfs_storage::take_storage_receipts());
    }

    #[doc(hidden)]
    pub fn finish_operation(&self) -> Vec<layerfs_storage::StorageReceipt> {
        layerfs_storage::take_storage_receipts()
    }

    #[doc(hidden)]
    pub fn attach_route(&self, route: MonitoredRoute) -> MonitorResult<()> {
        self.routes
            .lock()
            .map_err(|_| MonitorError::Integrity("route lock"))?
            .insert(route.id, route);
        Ok(())
    }

    #[doc(hidden)]
    pub fn detach_route(&self, id: BranchStoreId) -> MonitorResult<()> {
        self.routes
            .lock()
            .map_err(|_| MonitorError::Integrity("route lock"))?
            .remove(&id)
            .ok_or(MonitorError::NotFound)?;
        self.dedup
            .lock()
            .map_err(|_| MonitorError::Integrity("dedup lock"))?
            .remove(&id);
        Ok(())
    }

    fn database_snapshot(&self) -> MonitorResult<MonitorSnapshot> {
        let mut seen = BTreeSet::new();
        let mut databases = Vec::new();
        let routes = self
            .routes
            .lock()
            .map_err(|_| MonitorError::Integrity("route lock"))?;
        for route in routes.values() {
            push_database(
                "branch",
                route.branch.path(),
                route.branch.storage_snapshot()?,
                &mut seen,
                &mut databases,
            );
            if let Some(stack) = &route.stack {
                push_database(
                    "stack",
                    stack.path(),
                    stack.storage_snapshot()?,
                    &mut seen,
                    &mut databases,
                );
            }
            push_database(
                "layer",
                route.layer.path(),
                route.layer.storage_snapshot()?,
                &mut seen,
                &mut databases,
            );
        }
        Ok(MonitorSnapshot::Databases(databases))
    }
}

fn push_database(
    role: &str,
    path: &Path,
    storage: layerfs_storage::StoreStorageSnapshot,
    seen: &mut BTreeSet<(String, String)>,
    databases: &mut Vec<DatabaseSnapshot>,
) {
    let location = path.to_string_lossy().into_owned();
    if seen.insert((role.to_owned(), location.clone())) {
        databases.push(DatabaseSnapshot {
            role: role.to_owned(),
            location,
            storage,
        });
    }
}
