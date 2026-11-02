//! `microsandbox`'s profile.
use crate::options::{CPUS, DISK, IMAGE, MEMORY};
use crate::platform::{fuse, namespaces, worker, How, Profile, ANY_HOST};
use std::net::Ipv4Addr;

pub(crate) const PROFILE: Profile = Profile {
    name: "microsandbox",
    tool: Some("msb"),
    how: How::Created,
    hosts: ANY_HOST,
    backend: namespaces,
    kernel: fuse,
    executor: worker,
    worker_bind: Ipv4Addr::UNSPECIFIED,
    guest_stop: "1",
    options: &[CPUS, MEMORY, DISK, IMAGE],
    description: "a microsandbox microVM (libkrun)",
};
