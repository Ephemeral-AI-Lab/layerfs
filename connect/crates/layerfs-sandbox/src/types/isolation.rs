//! The isolation and execution contract of a Machine or a backend (#551):
//! how strong the boundary around a Sandbox is, whether work runs through a
//! worker or by remote exec, and whether it shares the caller's kernel. The
//! same Sandbox can sit behind any class; a class is a capability, never a
//! backend name.

/// How strong the boundary around a Sandbox is (#551 B0-144).
///
/// The variants are in strength order, weakest first, so a request is
/// satisfied by the same class or a stronger one and never downgraded.
/// `Vm` covers a microVM provider: the substrate is a provider detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IsolationClass {
    /// A process on the caller's host (host process or namespaces).
    Process,
    /// A container sharing the caller's kernel.
    Container,
    /// A virtual machine, with its own kernel: a microVM or a VM.
    Vm,
    /// A remote host reached over the network.
    Remote,
}

impl IsolationClass {
    /// Every class, weakest first.
    pub const ALL: [IsolationClass; 4] = [
        IsolationClass::Process,
        IsolationClass::Container,
        IsolationClass::Vm,
        IsolationClass::Remote,
    ];

    /// The stable lowercase name records and read models use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Process => "process",
            Self::Container => "container",
            Self::Vm => "vm",
            Self::Remote => "remote",
        }
    }

    /// The class named `name`, if it is one.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|class| class.as_str() == name)
    }

    /// The strength word the capability matrix uses.
    pub fn strength(self) -> &'static str {
        match self {
            Self::Process => "weak",
            Self::Container => "medium",
            Self::Vm => "strong",
            Self::Remote => "network",
        }
    }

    /// Whether a Sandbox of this class satisfies a request for `requested`:
    /// the same class or a stronger one. A weaker class is never a
    /// substitute; the caller answers `Unavailable` instead.
    pub fn satisfies(self, requested: Self) -> bool {
        self >= requested
    }
}

impl std::fmt::Display for IsolationClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where a Sandbox's work runs (#551 B0-146).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExecMode {
    /// Our worker runs in the guest, reachable through a machine endpoint.
    Worker,
    /// The provider runs each command through its own exec API.
    RemoteExec,
}

impl ExecMode {
    /// The stable name records and read models use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Worker => "worker",
            Self::RemoteExec => "remote-exec",
        }
    }

    /// The mode named `name`, if it is one.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "worker" => Some(Self::Worker),
            "remote-exec" => Some(Self::RemoteExec),
            _ => None,
        }
    }
}

impl std::fmt::Display for ExecMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whether a Sandbox shares the caller's kernel (#551 B0-144).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KernelScope {
    /// The caller's kernel (a host process or a container).
    Shared,
    /// Its own kernel in the same host (a VM).
    Separate,
    /// Its own host and kernel (a remote machine).
    SeparateHost,
}

impl KernelScope {
    /// The stable name records and read models use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Separate => "separate",
            Self::SeparateHost => "separate-host",
        }
    }

    /// The scope named `name`, if it is one.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "shared" => Some(Self::Shared),
            "separate" => Some(Self::Separate),
            "separate-host" => Some(Self::SeparateHost),
            _ => None,
        }
    }

    /// The scope a class runs with.
    pub fn of_class(class: IsolationClass) -> Self {
        match class {
            IsolationClass::Process | IsolationClass::Container => Self::Shared,
            IsolationClass::Vm => Self::Separate,
            IsolationClass::Remote => Self::SeparateHost,
        }
    }
}

impl std::fmt::Display for KernelScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
