//! `ssh`: a Linux host reached over ssh, connected and never created.
#[cfg(unix)]
mod adapter;
#[cfg(unix)]
mod connect;
#[cfg(unix)]
mod host_key;
#[cfg(unix)]
mod observe;
#[cfg(unix)]
mod power;
mod profile;
#[cfg(unix)]
pub(crate) use adapter::ADAPTER;
#[cfg(unix)]
pub(crate) use connect::{connect, disconnect};
#[cfg(unix)]
pub(crate) use observe::{daemon_forward, identity, machine_guest, status};
#[cfg(unix)]
pub(crate) use power::{start, stop};
pub(crate) use profile::PROFILE;
