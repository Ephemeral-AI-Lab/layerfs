//! `namespaces`'s backend adapter.
use crate::backend::adapter::BackendAdapter;
use crate::{IdleStop, IsolationModel, MountDriver, ProviderCapabilities, SearchPathPolicy};

pub(crate) struct NamespacesBackend;
pub(crate) static ADAPTER: NamespacesBackend = NamespacesBackend;

impl BackendAdapter for NamespacesBackend {
    fn stored_name(&self) -> &'static str {
        "namespaces"
    }
    fn description(&self) -> &'static str {
        "Sandboxes run in Linux namespaces and a cgroup"
    }
    /// The isolation boundary is the VM that holds this one kernel (one per
    /// Store); a Sandbox inside it owns its mount, PID and IPC namespaces and
    /// its cgroup, not a VM of its own.
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            schema_version: crate::CAPABILITY_SCHEMA_VERSION,
            name: "namespaces-linux",
            isolation: IsolationModel::MicroVm,
            mount_driver: MountDriver::Fuse,
            requires_image: true,
            search_path_policy: SearchPathPolicy::FixedContainer,
            supports_resource_limits: false,
            direct_store_access: true,
            idle_stop: IdleStop::Ours,
            // no backend built here snapshots a Sandbox
            snapshot: None,
        }
    }
}
