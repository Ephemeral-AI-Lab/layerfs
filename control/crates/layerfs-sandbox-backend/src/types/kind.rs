//! How a Sandbox runs on a machine (the portable data face; the runtime
//! methods live in the unix backend module).
/// How a Sandbox runs on a machine; callers never choose it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Host processes with loopback NFS (macOS).
    HostProcess,
    /// Linux namespaces and a cgroup.
    Namespaces,
}

impl Backend {
    pub const ALL: [Backend; 2] = [Backend::HostProcess, Backend::Namespaces];

    /// The value records and listings store and show.
    pub fn stored_name(self) -> &'static str {
        match self {
            Self::HostProcess => "native",
            Self::Namespaces => "namespaces",
        }
    }

    /// The backend whose stored name is `name`.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.stored_name() == name)
    }

    /// One line: how a Sandbox runs on this backend (the portable wording;
    /// the unix face adds its adapter-specific description).
    pub fn description(self) -> &'static str {
        match self {
            Self::HostProcess => "host processes with loopback NFS",
            Self::Namespaces => "Linux namespaces and a cgroup",
        }
    }
}
