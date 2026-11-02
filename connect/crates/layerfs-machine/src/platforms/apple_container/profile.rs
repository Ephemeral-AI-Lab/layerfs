//! `apple-container`'s profile.
use crate::options::{CPUS, DISK, IMAGE, MEMORY};
use crate::platform::{fuse, namespaces, worker, How, Profile, MACOS};
use std::net::Ipv4Addr;

pub(crate) const PROFILE: Profile = Profile {
    name: "apple-container",
    tool: Some("container"),
    how: How::Created,
    hosts: MACOS,
    backend: namespaces,
    kernel: fuse,
    executor: worker,
    worker_bind: Ipv4Addr::UNSPECIFIED,
    guest_stop: "init",
    options: &[CPUS, MEMORY, DISK, IMAGE],
    description: "an Apple container VM (Apple silicon)",
};
