//! `lima` (`limactl`, `vmType: vz`, macOS only).
#[cfg(unix)]
mod adapter;
#[cfg(unix)]
mod create;
#[cfg(unix)]
mod observe;
#[cfg(unix)]
mod power;
mod profile;
#[cfg(unix)]
mod template;
#[cfg(unix)]
pub(crate) use adapter::ADAPTER;
#[cfg(unix)]
pub(crate) use create::{create, remove};
#[cfg(unix)]
pub(crate) use observe::{daemon_forward, guest, status, tunnel};
#[cfg(unix)]
pub(crate) use power::{start, stop};
pub(crate) use profile::PROFILE;
