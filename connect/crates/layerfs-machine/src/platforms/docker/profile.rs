//! `docker`'s profile. Docker through WSL2 (Windows) is not designed.
use crate::platform::{fuse, worker, HostOs, How, Profile};
use layerfs_sandbox::Backend;
use std::net::Ipv4Addr;

pub(crate) const PROFILE: Profile = Profile {
    name: "docker",
    tool: Some("docker"),
    how: How::BuiltIn,
    hosts: &[HostOs::Macos, HostOs::Linux],
    backend,
    kernel: fuse,
    executor: worker,
    worker_bind: Ipv4Addr::UNSPECIFIED,
    options: &[],
    description: "Docker's kernel (Docker Desktop's VM, or the Linux host)",
};

fn backend(_: HostOs) -> Option<Backend> {
    Some(Backend::Docker)
}
