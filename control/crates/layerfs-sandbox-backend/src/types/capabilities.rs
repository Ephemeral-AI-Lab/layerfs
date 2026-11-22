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
/// What separates a Sandbox's commands from what else runs, as far as the
/// backend itself knows: it is the same on every platform, so it never
/// counts a VM or container that the machine around the kernel may add.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IsolationModel {
    Container,
    /// The caller's own processes, on its kernel and network.
    HostProcess,
    MicroVm,
    /// Linux mount, PID and IPC namespaces and a cgroup on the kernel that
    /// runs the Sandbox, which may be the host's: no network namespace (its
    /// commands use that kernel's network, the host's on a Linux host's
    /// `native` or `docker` machine) and no kernel of its own.
    Namespaces,
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
