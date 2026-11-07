//! `host_process`'s backend adapter; stored and shown as `"native"` (G6).
use crate::backend::adapter::BackendAdapter;
use crate::{IsolationModel, MountDriver, ProviderCapabilities, SearchPathPolicy};

pub(crate) struct HostProcessBackend;
pub(crate) static ADAPTER: HostProcessBackend = HostProcessBackend;

impl BackendAdapter for HostProcessBackend {
    fn stored_name(&self) -> &'static str {
        "native"
    }
    fn description(&self) -> &'static str {
        "Sandboxes run as this user's processes, served over loopback NFS"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            name: "native-macos",
            isolation: IsolationModel::HostProcess,
            mount_driver: MountDriver::LoopbackNfs,
            requires_image: false,
            search_path_policy: SearchPathPolicy::InheritHost,
            supports_resource_limits: false,
            direct_store_access: true,
        }
    }
}
