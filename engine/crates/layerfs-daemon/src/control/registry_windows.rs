//! The Windows control registry: the Service face over the same
//! Store, with the in-process WinFsp Ready state where the unix
//! Service holds its kernel session or NFS export (#454).
use super::Failure;
use layerfs_history::WorkspaceId;
use layerfs_store_service::store::Store;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// One admitted Workspace control slot: its binding, or the Ready
/// mount the WinFsp face attached.
pub(super) enum Entry {
    /// Its bind is admitted; no mount yet.
    Bound,
    /// The WinFsp mount answers its filesystem calls (A2).
    Ready(Box<super::kernel::WinReady>),
}

/// The Windows Service: the same Store and capacity admission, the
/// entries map with the WinFsp Ready face.
pub struct Service {
    pub(crate) store: Arc<Store>,
    entries: Mutex<BTreeMap<WorkspaceId, Entry>>,
    capacity: usize,
}

impl Service {
    /// Binds control at its configured capacity.
    pub fn new(store: Arc<Store>, capacity: usize) -> Self {
        Self {
            store,
            entries: Mutex::new(BTreeMap::new()),
            capacity,
        }
    }

    /// One admission attempt: the capacity and duplicate checks the
    /// unix registry makes, before any effect.
    pub(super) fn admit(&self, workspace: WorkspaceId) -> Result<(), Failure> {
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        if entries.contains_key(&workspace) {
            return Err(Failure::Rejected(
                layerfs_bridge::control::ControlCode::Busy,
                "Workspace incarnation already admitted",
            ));
        }
        if entries.len() == self.capacity {
            return Err(Failure::Rejected(
                layerfs_bridge::control::ControlCode::Capacity,
                "daemon Workspace capacity",
            ));
        }
        entries.insert(workspace, Entry::Bound);
        Ok(())
    }
}

impl Service {
    /// Attaches the admitted Workspace over WinFsp at one directory under
    /// `mounts`: the real mount needs the driver; the entry move and the
    /// mount are one step once it is installed.
    pub(super) fn attach_ready(
        &self,
        workspace: WorkspaceId,
        mounts: &std::path::Path,
        services: std::sync::Arc<dyn layerfs_mount_ports::ports::MountServices>,
    ) -> Result<(), Failure> {
        let point = mounts.join(format!("ws-{:?}", workspace));
        let mounted =
            layerfs_winfsp::mount_workspace(point.to_string_lossy().as_ref(), services)
                .map_err(|error| Failure::Rejected(
                    layerfs_bridge::control::ControlCode::Invalid,
                    "winfsp mount failed",
                ))?;
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        entries.insert(
            workspace,
            Entry::Ready(Box::new(super::kernel::WinReady {
                mounted,
                point,
            })),
        );
        Ok(())
    }
}
