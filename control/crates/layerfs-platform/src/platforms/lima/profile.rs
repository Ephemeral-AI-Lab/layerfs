//! `lima`'s profile.
use crate::options::{CPUS, DISK, IMAGE, MEMORY};
use crate::platform::{fuse, namespaces, worker, How, Profile, MACOS};
use std::net::Ipv4Addr;

pub(crate) const PROFILE: Profile = Profile {
    name: "lima",
    tool: Some("limactl"),
    how: How::Created,
    hosts: MACOS,
    backend: namespaces,
    kernel: fuse,
    executor: worker,
    worker_bind: Ipv4Addr::LOCALHOST,
    options: &[CPUS, MEMORY, DISK, IMAGE],
    description: "a Lima VM (vz)",
};
