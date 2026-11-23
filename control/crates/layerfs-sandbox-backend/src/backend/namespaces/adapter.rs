//! `namespaces`'s backend adapter.
use crate::backend::adapter::BackendAdapter;
use crate::{IsolationModel, MountDriver, ProviderCapabilities, SearchPathPolicy};

pub(crate) struct NamespacesBackend;
pub(crate) static ADAPTER: NamespacesBackend = NamespacesBackend;

impl BackendAdapter for NamespacesBackend {
    fn stored_name(&self) -> &'static str {
        "namespaces"
    }
    fn description(&self) -> &'static str {
        "Sandboxes run in Linux namespaces and a cgroup"
    }
    /// A Sandbox owns its mount, PID and IPC namespaces, its cgroup and its
    /// commands' network namespace on the kernel that runs it: a VM's on
    /// `lima` and `apple-container`, the host's own on a Linux host's
    /// `native` and `docker` machines. No VM of its own, so the claim is
    /// `Namespaces`, whatever the machine around it adds.
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            name: "namespaces-linux",
            isolation: IsolationModel::Namespaces,
            mount_driver: MountDriver::Fuse,
            requires_image: true,
            search_path_policy: SearchPathPolicy::FixedContainer,
            supports_resource_limits: false,
            direct_store_access: true,
        }
    }
}
