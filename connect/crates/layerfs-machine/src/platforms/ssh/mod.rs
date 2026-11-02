//! `ssh`: a Linux host reached over ssh, connected and never created.
mod adapter;
mod connect;
mod host_key;
mod observe;
mod power;
mod profile;
pub(crate) use adapter::ADAPTER;
pub(crate) use connect::{connect, disconnect};
pub(crate) use observe::{identity, machine_guest, status};
pub(crate) use power::{start, stop};
pub(crate) use profile::PROFILE;
