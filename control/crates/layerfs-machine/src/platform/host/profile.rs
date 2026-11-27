//! `host`'s profile: Sandboxes on the host's own OS, with no VM. The
//! machine is not the host: its executor, loopback and backend are what
//! make it special, and they differ by host (Windows has no backend).
use {crate::table::profile::{ExecutorKind, HostOs, How, Profile, ANY_HOST}, layerfs_protocol::KernelRoute};
use crate::table::backend as Backend;
use std::net::Ipv4Addr;

pub(crate) const PROFILE: Profile = Profile {
    name: "host",
    tool: None,
    how: How::BuiltIn,
    hosts: ANY_HOST,
    backend,
    kernel,
    executor,
    worker_bind: Ipv4Addr::UNSPECIFIED,
    options: &[],
    description: "the host's own OS, with no VM",
};

/// macOS: this user's processes; Linux: namespaces; Windows holds only
/// records (§13 O5).
fn backend(host: HostOs) -> Option<Backend> {
    match host {
        HostOs::Macos => Some(Backend::Process),
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
fn executor(host: HostOs) -> ExecutorKind {
    match host {
        HostOs::Windows => ExecutorKind::Worker,
        _ => ExecutorKind::Local,
    }
}
