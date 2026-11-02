//! `ssh`: a Linux host reached over ssh, connected and never created.
mod connect;
mod host_key;
mod observe;
mod power;
pub(crate) use connect::{connect, disconnect};
pub(crate) use observe::{identity, machine_guest, status};
pub(crate) use power::{start, stop};
