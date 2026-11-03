//! `ssh`'s profile.
use crate::options::{opt, OptKind};
use crate::platform::{fuse, namespaces, worker, How, Profile, ANY_HOST};
use std::net::Ipv4Addr;

pub(crate) const PROFILE: Profile = Profile {
    name: "ssh",
    tool: Some("ssh"),
    how: How::Connected,
    hosts: ANY_HOST,
    backend: namespaces,
    kernel: fuse,
    executor: worker,
    worker_bind: Ipv4Addr::LOCALHOST,
    options: &[
        opt("user", OptKind::Text),
        opt("port", OptKind::Int),
        opt("identity", OptKind::Text),
        opt("known_hosts", OptKind::Text),
        opt("root", OptKind::Text),
    ],
    description: "a Linux host over ssh, connected and never created",
};
