//! `docker`'s profile: its worker container runs `namespaces` Sandboxes as
//! a VM does (D-8). Docker through WSL2 (Windows) is not designed.
use crate::platform::{fuse, namespaces, worker, HostOs, How, Profile};
use std::net::Ipv4Addr;

pub(crate) const PROFILE: Profile = Profile {
    name: "docker",
    tool: Some("docker"),
    how: How::BuiltIn,
    hosts: &[HostOs::Macos, HostOs::Linux],
    backend: namespaces,
    kernel: fuse,
    executor: worker,
    worker_bind: Ipv4Addr::UNSPECIFIED,
    options: &[],
    description: "Docker's kernel (Docker Desktop's VM, or the Linux host)",
};
