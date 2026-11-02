//! The seven machine kinds and the one table that pairs each with its Sandbox
//! backend, its route and its guest worker's listen address (H §7.10.1).
use std::net::Ipv4Addr;

/// Which kernel Sandboxes run on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Native,
    Docker,
    AppleContainer,
    Microsandbox,
    Lima,
    Firecracker,
    Ssh,
}

/// How a Sandbox runs on a machine; callers never choose it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Host processes with loopback NFS (macOS).
    Native,
    /// Linux namespaces and a cgroup.
    Namespaces,
    /// Docker containers.
    Docker,
}

/// Where a machine's Store calls run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// In the caller's own process.
    InProcess,
    /// In the machine's worker, reached through a host loopback port.
    Worker,
}

/// The operating system of the host the caller runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostOs {
    Linux,
    Macos,
    Windows,
    Other,
}

impl HostOs {
    /// The host this build runs on.
    pub fn current() -> Self {
        if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Other
        }
    }
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Native,
        Kind::Docker,
        Kind::AppleContainer,
        Kind::Microsandbox,
        Kind::Lima,
        Kind::Firecracker,
        Kind::Ssh,
    ];

    /// The kind's name as callers write it (`layerfs machine create dev lima`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Docker => "docker",
            Self::AppleContainer => "apple-container",
            Self::Microsandbox => "microsandbox",
            Self::Lima => "lima",
            Self::Firecracker => "firecracker",
            Self::Ssh => "ssh",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// `native` and `docker`: one each, never created or removed.
    pub fn built_in(self) -> bool {
        matches!(self, Self::Native | Self::Docker)
    }

    /// The kinds `machine create` makes: each its own VM.
    pub fn created(self) -> bool {
        matches!(
            self,
            Self::AppleContainer | Self::Microsandbox | Self::Lima | Self::Firecracker
        )
    }

    /// The provider CLI the kind runs, if any.
    pub fn tool(self) -> Option<&'static str> {
        match self {
            Self::Native => None,
            Self::Docker => Some("docker"),
            Self::AppleContainer => Some("container"),
            Self::Microsandbox => Some("msb"),
            Self::Lima | Self::Firecracker => Some("limactl"),
            Self::Ssh => Some("ssh"),
        }
    }

    /// The Sandbox backend on this kind of machine, or none (`native` on
    /// Windows holds only records, §13 O5).
    pub fn backend(self, host: HostOs) -> Option<Backend> {
        match self {
            Self::Native => match host {
                HostOs::Macos => Some(Backend::Native),
                HostOs::Linux => Some(Backend::Namespaces),
                HostOs::Windows | HostOs::Other => None,
            },
            Self::Docker => Some(Backend::Docker),
            _ => Some(Backend::Namespaces),
        }
    }

    /// Where Store calls for a sqlite Store on this machine run: in-process
    /// for `native` (except Windows, whose Stores are in the WSL worker) and
    /// for `docker` on Linux; the machine's worker otherwise.
    pub fn route(self, host: HostOs) -> Route {
        match (self, host) {
            (Self::Native, HostOs::Windows) => Route::Worker,
            (Self::Native, _) => Route::InProcess,
            _ => Route::Worker,
        }
    }

    /// The guest address the worker binds. Loopback wherever the provider's
    /// forward reaches the guest's loopback (Lima's port forwarding and
    /// `ssh -L`); every interface where the forward arrives on the guest's
    /// network (`apple-container` and `microsandbox` publish a port,
    /// `firecracker`'s forward ends at its tap address, `docker` publishes a
    /// container port). The worker's secret guards it either way.
    pub fn worker_bind(self) -> Ipv4Addr {
        match self {
            Self::Lima | Self::Ssh => Ipv4Addr::LOCALHOST,
            _ => Ipv4Addr::UNSPECIFIED,
        }
    }

    /// What the worker does to its guest once idle
    /// (`LAYERFS_WORKER_POWEROFF`): nothing on an ssh host (not ours to
    /// stop), a reset in a Firecracker microVM (its VMM exits on a reset and
    /// ignores an ACPI power-off), SIGTERM to an apple-container's main
    /// process (it has no init or `CAP_SYS_BOOT`), a power-off in every
    /// other VM.
    pub fn guest_stop(self) -> &'static str {
        match self {
            Self::Ssh => "0",
            Self::Firecracker => "reboot",
            Self::AppleContainer => "init",
            _ => "1",
        }
    }
}
