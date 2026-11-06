//! How a Sandbox runs on a machine (`layerfs-platform`'s `Platform::backend`);
//! callers never choose it.
use super::adapter::adapter;
use crate::ProviderCapabilities;

/// How a Sandbox runs on a machine; callers never choose it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Host processes with loopback NFS (macOS).
    HostProcess,
    /// Linux namespaces and a cgroup.
    Namespaces,
    /// Docker containers.
    Docker,
}

impl Backend {
    pub const ALL: [Backend; 3] = [Backend::Docker, Backend::HostProcess, Backend::Namespaces];

    /// The value records and listings store and show: `"docker"`,
    /// `"native"` (for `HostProcess`, G6) or `"namespaces"`.
    pub fn stored_name(self) -> &'static str {
        adapter(self).stored_name()
    }

    /// The backend whose stored name is `name`.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.stored_name() == name)
    }

    /// One line: how a Sandbox runs on this backend.
    pub fn description(self) -> &'static str {
        adapter(self).description()
    }

    /// What this backend supports.
    pub fn capabilities(self) -> ProviderCapabilities {
        adapter(self).capabilities()
    }
}
