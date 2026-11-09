//! `native`'s profile.
use crate::platform::{ExecutorKind, HostOs, How, KernelRoute, Profile, ANY_HOST};
use layerfs_sandbox_backend::Backend;
use std::net::Ipv4Addr;

pub(crate) const PROFILE: Profile = Profile {
    name: "native",
    tool: None,
    how: How::BuiltIn,
    hosts: ANY_HOST,
    backend,
    kernel,
    executor,
    worker_bind: Ipv4Addr::UNSPECIFIED,
    options: &[],
    description: "this host",
};

/// macOS: this user's processes; Linux: namespaces; Windows holds only
/// records (§13 O5).
fn backend(host: HostOs) -> Option<Backend> {
    match host {
        HostOs::Macos => Some(Backend::HostProcess),
        HostOs::Linux => Some(Backend::Namespaces),
        HostOs::Windows | HostOs::Other => None,
    }
}

/// Loopback NFS on macOS, FUSE on Linux.
fn kernel(host: HostOs) -> Option<KernelRoute> {
    match host {
        HostOs::Macos => Some(KernelRoute::Nfs),
        HostOs::Linux => Some(KernelRoute::Fuse),
        HostOs::Windows | HostOs::Other => None,
    }
}

/// In the caller's process, except on Windows, whose Stores are in the WSL
/// worker.
fn executor(_host: HostOs) -> ExecutorKind {
    ExecutorKind::Local
}
