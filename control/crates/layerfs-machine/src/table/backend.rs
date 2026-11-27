//! How a Sandbox runs on a machine (`layerfs-platform`'s `Platform::backend`);
//! callers never choose it.
/// How a Sandbox runs on a machine; callers never choose it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Host processes with loopback NFS (macOS).
    Process,
    /// Linux namespaces and a cgroup.
    Namespaces,
}

impl Backend {
    pub const ALL: [Backend; 2] = [Backend::Process, Backend::Namespaces];

    /// The value records and listings store and show: `"process"` (for
    /// `Process`, G6) or `"namespaces"`.
    pub fn stored_name(self) -> &'static str {
        match self {
            Backend::Process => "process",
            Backend::Namespaces => "namespaces",
        }
    }

    /// The backend whose stored name is `name`.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.stored_name() == name)
    }

    /// One line: how a Sandbox runs on this backend.
    pub fn description(self) -> &'static str {
        match self {
            Backend::Process => "Sandboxes run as this user's processes, served over loopback NFS",
            Backend::Namespaces => "Sandboxes run in Linux namespaces and a cgroup",
        }
    }
}
