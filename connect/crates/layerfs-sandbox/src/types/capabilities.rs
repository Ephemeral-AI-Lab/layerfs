//! Explicit runtime capabilities; these describe support, not qualification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCapabilities {
    pub name: &'static str,
    pub isolation: IsolationModel,
    pub mount_driver: MountDriver,
    pub requires_image: bool,
    pub search_path_policy: SearchPathPolicy,
    pub supports_resource_limits: bool,
    pub direct_store_access: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IsolationModel {
    Container,
    HostProcess,
    MicroVm,
    RemoteCloud,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MountDriver {
    Fuse,
    LoopbackNfs,
    SharedFs,
    None,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchPathPolicy {
    FixedContainer,
    InheritHost,
}
