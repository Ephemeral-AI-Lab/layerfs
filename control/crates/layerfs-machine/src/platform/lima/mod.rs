//! `lima` (`limactl`, `vmType: vz`, macOS only).
mod adapter;
pub(crate) mod lifecycle;
pub(crate) mod name_limit;
mod profile;
pub(crate) mod status;
mod template;
pub(crate) use adapter::ADAPTER;
pub(crate) use lifecycle::{create, remove};
pub(crate) use lifecycle::{start, stop};
pub(crate) use profile::PROFILE;
pub(crate) use status::{daemon_forward, guest, status};
