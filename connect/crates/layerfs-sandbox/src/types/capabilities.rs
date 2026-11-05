//! Explicit runtime capabilities; these describe support, not qualification.
//! One typed declaration per backend (#551 B0-145, B0-152): its isolation
//! class and kernel, its mount driver, whether its work runs through our
//! worker, how an idle Sandbox stops, and whether it can snapshot. The
//! `schema_version` versions the declaration itself; a consumer reads it
//! before relying on a field and never guesses an absent one.
use super::{IsolationClass, KernelScope};

/// The backend-capability schema this build writes (#551 B0-152). A backend
/// that answers a newer version is read for the fields it still shares.
pub const CAPABILITY_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCapabilities {
    /// The capability schema this declaration follows (#551 B0-152).
    pub schema_version: u32,
    pub name: &'static str,
    pub isolation: IsolationModel,
    pub mount_driver: MountDriver,
    pub requires_image: bool,
    pub search_path_policy: SearchPathPolicy,
    pub supports_resource_limits: bool,
    pub direct_store_access: bool,
    /// How an idle Sandbox on this backend stops (#551 B0-145): by us, by
    /// the provider, or never.
    pub idle_stop: IdleStop,
    /// The provider's snapshot support (#551 B0-145), when it has one.
    pub snapshot: Option<SnapshotCapability>,
}

/// Who ends an idle Sandbox (#551 B0-145).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdleStop {
    /// Our worker or daemon ends it.
    Ours,
    /// The provider ends it.
    Provider,
    /// Nothing ends it; it runs until a caller closes it.
    None,
}

impl IdleStop {
    /// The stable lowercase name records and read models use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ours => "ours",
            Self::Provider => "provider",
            Self::None => "none",
        }
    }
}

/// A provider's snapshot support, as it declares it (#551 B0-145): the
/// ceiling, its lifetime and whether it is still beta.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotCapability {
    pub max_bytes: u64,
    pub ttl_secs: u64,
    pub beta: bool,
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

impl MountDriver {
    /// The stable lowercase name records and read models use (#551 B10-174).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fuse => "fuse",
            Self::LoopbackNfs => "loopback-nfs",
            Self::SharedFs => "shared-fs",
            Self::None => "none",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchPathPolicy {
    FixedContainer,
    InheritHost,
}

impl IsolationModel {
    /// The #551 class this backend declares.
    pub fn isolation_class(self) -> IsolationClass {
        match self {
            Self::Container => IsolationClass::Container,
            Self::HostProcess => IsolationClass::Process,
            Self::MicroVm => IsolationClass::Vm,
            Self::RemoteCloud => IsolationClass::Remote,
        }
    }

    /// The kernel scope this backend declares (#551).
    pub fn kernel_scope(self) -> KernelScope {
        KernelScope::of_class(self.isolation_class())
    }
}

impl ProviderCapabilities {
    /// The isolation class this backend declares (#551 B0-145).
    pub fn isolation_class(&self) -> IsolationClass {
        self.isolation.isolation_class()
    }

    /// The kernel scope this backend declares (#551 B0-145).
    pub fn kernel_scope(&self) -> KernelScope {
        self.isolation.kernel_scope()
    }
}
