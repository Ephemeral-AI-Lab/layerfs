//! `firecracker`'s profile.
use crate::options::{opt, OptKind, CPUS, DISK, IMAGE, MEMORY};
use crate::platform::{fuse, namespaces, worker, How, Profile, MACOS};
use std::net::Ipv4Addr;

pub(crate) const PROFILE: Profile = Profile {
    name: "firecracker",
    tool: Some("limactl"),
    how: How::Created,
    hosts: MACOS,
    backend: namespaces,
    kernel: fuse,
    executor: worker,
    worker_bind: Ipv4Addr::UNSPECIFIED,
    guest_stop: "reboot",
    options: &[CPUS, MEMORY, DISK, IMAGE, opt("host", OptKind::Name)],
    description: "a Firecracker microVM in a nested lima machine",
};
