//! The five machine platforms and the one table that describes each: its
//! [`Profile`], one `PROFILE` per platform folder (H §7.10.1, #535 §6.2).
//! [`Platform::profile`] is the one match; every other per-platform value is
//! read from the profile.
use crate::options::OptSpec;
use crate::platforms::{apple_container, docker, firecracker, lima, native};
pub use layerfs_bridge::KernelRoute;
use layerfs_sandbox_backend::Backend;
use std::net::Ipv4Addr;

/// Which kernel Sandboxes run on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Platform {
    Native,
    Docker,
    AppleContainer,
    Lima,
    Firecracker,
}

/// Where a machine's Store calls run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// In the caller's own process.
    InProcess,
    /// In the machine's worker, reached through a host loopback port.
    Worker,
}

/// Who runs a sqlite Store's calls for a machine (#535 §6.2, G8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutorKind {
    /// The caller's own process.
    Local,
    /// The machine's worker.
    Worker,
}

/// How a machine of a platform comes to exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    /// One each, never created or removed (`native`, `docker`).
    BuiltIn,
    /// Made and removed by `machine create` / `machine remove`.
    Created,
}

/// The operating system of the host the caller runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostOs {
    Linux,
    Macos,
    Windows,
    Other,
}

/// One platform's facts (#535 §6.2), read through [`Platform::profile`].
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    /// The name callers write (`layerfs machine create dev lima`).
    pub name: &'static str,
    /// The provider CLI the platform runs, if any.
    pub tool: Option<&'static str>,
    pub how: How,
    /// The hosts a machine of this platform can be on.
    pub hosts: &'static [HostOs],
    /// The Sandbox backend on this platform for a host, or none.
    pub backend: fn(HostOs) -> Option<Backend>,
    /// The kernel route its Sandboxes' daemons serve, or none.
    pub kernel: fn(HostOs) -> Option<KernelRoute>,
    /// Who runs a sqlite Store's calls.
    pub executor: fn(HostOs) -> ExecutorKind,
    /// The guest address the worker binds.
    pub worker_bind: Ipv4Addr,
    /// The options `machine create` takes.
    pub options: &'static [OptSpec],
    /// One line: what a machine of this platform is.
    pub description: &'static str,
}

impl How {
    /// Its stable lowercase name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "built_in",
            Self::Created => "created",
        }
    }
}

impl HostOs {
    /// The hosts LayerFS names; `Other` is none of them.
    pub const NAMED: [HostOs; 3] = [HostOs::Macos, HostOs::Linux, HostOs::Windows];

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

    /// Its stable lowercase name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Other => "other",
        }
    }
}

/// Every named host.
pub(crate) const ANY_HOST: &[HostOs] = &HostOs::NAMED;
/// macOS only.
pub(crate) const MACOS: &[HostOs] = &[HostOs::Macos];

/// A VM machine's backend: Linux namespaces in its guest.
pub(crate) fn namespaces(_: HostOs) -> Option<Backend> {
    Some(Backend::Namespaces)
}

/// A FUSE kernel route on every host.
pub(crate) fn fuse(_: HostOs) -> Option<KernelRoute> {
    Some(KernelRoute::Fuse)
}

/// The machine's worker on every host.
pub(crate) fn worker(_: HostOs) -> ExecutorKind {
    ExecutorKind::Worker
}

impl Platform {
    pub const ALL: [Platform; 5] = [
        Platform::Native,
        Platform::Docker,
        Platform::AppleContainer,
        Platform::Lima,
        Platform::Firecracker,
    ];

    /// The platform's profile: the one match over platforms for its facts.
    pub fn profile(self) -> &'static Profile {
        match self {
            Self::Native => &native::PROFILE,
            Self::Docker => &docker::PROFILE,
            Self::AppleContainer => &apple_container::PROFILE,
            Self::Lima => &lima::PROFILE,
            Self::Firecracker => &firecracker::PROFILE,
        }
    }

    /// The kind's name as callers write it (`layerfs machine create dev lima`).
    pub fn name(self) -> &'static str {
        self.profile().name
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// `native` and `docker`: one each, never created or removed.
    pub fn built_in(self) -> bool {
        self.profile().how == How::BuiltIn
    }

    /// The kinds `machine create` makes: each its own VM.
    pub fn created(self) -> bool {
        self.profile().how == How::Created
    }

    /// The provider CLI the kind runs, if any.
    pub fn tool(self) -> Option<&'static str> {
        self.profile().tool
    }

    /// The Sandbox backend on this kind of machine, or none (`native` on
    /// Windows holds only records, §13 O5).
    pub fn backend(self, host: HostOs) -> Option<Backend> {
        (self.profile().backend)(host)
    }

    /// The kernel route its Sandboxes' daemons serve on `host`, or none.
    pub fn kernel(self, host: HostOs) -> Option<KernelRoute> {
        (self.profile().kernel)(host)
    }

    /// Who runs a sqlite Store's calls on this machine (G8).
    pub fn executor(self, host: HostOs) -> ExecutorKind {
        (self.profile().executor)(host)
    }

    /// Where Store calls for a sqlite Store on this machine run: in-process
    /// for `native` (except Windows, whose Stores are in the WSL worker);
    /// the machine's worker otherwise, `docker` on every host included (G8).
    pub fn route(self, host: HostOs) -> Route {
        match self.executor(host) {
            ExecutorKind::Local => Route::InProcess,
            ExecutorKind::Worker => Route::Worker,
        }
    }

    /// The guest address the worker binds. Loopback wherever the provider's
    /// forward reaches the guest's loopback (Lima's port forwarding); every
    /// interface where the forward arrives on the guest's network
    /// (`apple-container` publishes a port,
    /// `firecracker`'s forward ends at its tap address, `docker` publishes a
    /// container port). The worker's secret guards it either way.
    pub fn worker_bind(self) -> Ipv4Addr {
        self.profile().worker_bind
    }
}
