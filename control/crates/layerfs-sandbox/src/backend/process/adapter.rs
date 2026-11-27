//! The `process` backend's adapter (`Backend::Process`); stored and shown as
//! `"process"` (G6).
use crate::backend::adapter::BackendAdapter;
use crate::{IsolationModel, MountDriver, ProviderCapabilities, SearchPathPolicy};

pub(crate) struct NativeBackend;
pub(crate) static ADAPTER: NativeBackend = NativeBackend;

impl BackendAdapter for NativeBackend {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            name: "process-macos",
            isolation: IsolationModel::HostProcess,
            mount_driver: MountDriver::LoopbackNfs,
            requires_image: false,
            search_path_policy: SearchPathPolicy::InheritHost,
            supports_resource_limits: false,
            direct_store_access: true,
        }
    }
}
