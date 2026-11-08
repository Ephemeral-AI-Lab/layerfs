//! `lima` (`limactl`, `vmType: vz`, macOS only).
mod adapter;
mod create;
mod observe;
mod power;
mod profile;
mod template;
pub(crate) use adapter::ADAPTER;
pub(crate) use create::{create, remove};
pub(crate) use observe::{daemon_forward, guest, status};
pub(crate) use power::{start, stop};
pub(crate) use profile::PROFILE;
