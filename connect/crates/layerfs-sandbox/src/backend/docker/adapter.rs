//! `docker`'s backend adapter.
use crate::backend::adapter::BackendAdapter;
use crate::{IsolationModel, MountDriver, ProviderCapabilities, SearchPathPolicy};

pub(crate) struct DockerBackend;
pub(crate) static ADAPTER: DockerBackend = DockerBackend;

impl BackendAdapter for DockerBackend {
    fn stored_name(&self) -> &'static str {
        "docker"
    }
    fn description(&self) -> &'static str {
        "Sandboxes run in Docker containers"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            name: "docker",
            isolation: IsolationModel::Container,
            mount_driver: MountDriver::Fuse,
            requires_image: true,
            search_path_policy: SearchPathPolicy::FixedContainer,
            supports_resource_limits: false,
            direct_store_access: true,
        }
    }
}
